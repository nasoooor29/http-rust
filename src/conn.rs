use crate::https::StatusCode;
use crate::router::PendingRequest;
use crate::router::ReadOutcome;

#[derive(Debug)]
pub struct Conn {
    pub local_port: u16,
    pub in_buf: Vec<u8>,
    pub out_buf: Vec<u8>,
    pub state: ConnState,
}

#[derive(Debug)]
pub enum ConnState {
    ReadingHeaders,
    ReadingBodyContentLength {
        header_end: usize,
        content_length: usize,
    },
    ReadingBodyChunked {
        header_end: usize,
    },
    Responding,
}

enum BodyFraming {
    ContentLength(usize),
    Chunked,
}

impl Conn {
    pub fn read_outcome(&mut self, new_bytes: &[u8]) -> ReadOutcome {
        self.in_buf.extend_from_slice(new_bytes);

        match self.state {
            ConnState::ReadingHeaders => self.read_headers(),
            ConnState::ReadingBodyContentLength {
                header_end,
                content_length,
            } => self.read_body_content_length(header_end, content_length),
            ConnState::ReadingBodyChunked { header_end } => {
                self.read_body_chunked(header_end)
            }
            ConnState::Responding => ReadOutcome::Pending,
        }
    }

    fn read_headers(&mut self) -> ReadOutcome {
        let Some(header_end) = self.find_header_end() else {
            return ReadOutcome::Pending;
        };

        let framing = match Self::parse_body_framing(&self.in_buf[..header_end])
        {
            Ok(v) => v,
            Err(reason) => {
                return ReadOutcome::Error {
                    status: StatusCode::BadRequest,
                    reason,
                };
            }
        };

        match framing {
            BodyFraming::ContentLength(0) => ReadOutcome::Ready(
                self.build_pending_request(header_end, Vec::new()),
            ),
            BodyFraming::ContentLength(content_length) => {
                self.state = ConnState::ReadingBodyContentLength {
                    header_end,
                    content_length,
                };
                self.read_body_content_length(header_end, content_length)
            }
            BodyFraming::Chunked => {
                self.state = ConnState::ReadingBodyChunked { header_end };
                self.read_body_chunked(header_end)
            }
        }
    }

    fn read_body_content_length(
        &mut self,
        header_end: usize,
        content_length: usize,
    ) -> ReadOutcome {
        let total_len = header_end + content_length;
        if self.in_buf.len() < total_len {
            return ReadOutcome::Pending;
        }

        ReadOutcome::Ready(self.build_pending_request(
            header_end,
            self.in_buf[header_end..total_len].to_vec(),
        ))
    }

    fn read_body_chunked(&mut self, header_end: usize) -> ReadOutcome {
        let body_and_trailers = &self.in_buf[header_end..];
        let (decoded_body, _consumed) =
            match Self::decode_chunked_body(body_and_trailers) {
                Ok(Some(v)) => v,
                Ok(None) => return ReadOutcome::Pending,
                Err(reason) => {
                    return ReadOutcome::Error {
                        status: StatusCode::BadRequest,
                        reason,
                    };
                }
            };

        ReadOutcome::Ready(self.build_pending_request(header_end, decoded_body))
    }

    fn build_pending_request(
        &mut self,
        header_end: usize,
        body_bytes: Vec<u8>,
    ) -> PendingRequest {
        PendingRequest {
            header_bytes: self.in_buf[..header_end].to_vec(),
            body_bytes,
            local_port: self.local_port,
        }
    }

    fn find_header_end(&mut self) -> Option<usize> {
        self.in_buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| i + 4)
    }

    fn parse_body_framing(header_bytes: &[u8]) -> Result<BodyFraming, String> {
        let text = std::str::from_utf8(header_bytes)
            .map_err(|_| "request headers are not valid UTF-8".to_string())?;
        let mut lines = text.split("\r\n");

        let _ = lines
            .next()
            .ok_or_else(|| "missing request line".to_string())?;

        let mut content_length: Option<usize> = None;
        let mut transfer_encoding: Option<String> = None;

        for line in lines {
            if line.is_empty() {
                break;
            }

            let Some((name, value)) = line.split_once(':') else {
                continue;
            };

            if !name.eq_ignore_ascii_case("Content-Length") {
                if name.eq_ignore_ascii_case("Transfer-Encoding") {
                    if transfer_encoding.is_some() {
                        return Err(
                            "duplicate Transfer-Encoding header".to_string()
                        );
                    }
                    transfer_encoding = Some(value.trim().to_ascii_lowercase());
                }
                continue;
            }

            if content_length.is_some() {
                return Err("duplicate Content-Length header".to_string());
            }

            let parsed = value.trim().parse::<usize>().map_err(|_| {
                "Content-Length must be a non-negative integer".to_string()
            })?;
            content_length = Some(parsed);
        }

        if let Some(te) = transfer_encoding {
            if content_length.is_some() {
                return Err(
                    "Transfer-Encoding and Content-Length cannot be combined"
                        .to_string(),
                );
            }

            let codings: Vec<&str> = te
                .split(',')
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .collect();

            if codings.is_empty() {
                return Err(
                    "Transfer-Encoding header cannot be empty".to_string()
                );
            }

            if codings.iter().any(|c| *c != "chunked") {
                return Err(
                    "only chunked Transfer-Encoding is supported".to_string()
                );
            }

            return Ok(BodyFraming::Chunked);
        }

        Ok(BodyFraming::ContentLength(content_length.unwrap_or(0)))
    }

    fn decode_chunked_body(
        raw: &[u8],
    ) -> Result<Option<(Vec<u8>, usize)>, String> {
        let mut pos = 0usize;
        let mut out = Vec::new();

        loop {
            let Some(line_end_rel) =
                raw[pos..].windows(2).position(|w| w == b"\r\n")
            else {
                return Ok(None);
            };
            let line_end = pos + line_end_rel;
            let size_line = &raw[pos..line_end];

            let size_text = std::str::from_utf8(size_line).map_err(|_| {
                "chunk size line is not valid UTF-8".to_string()
            })?;
            let size_token = size_text
                .split_once(';')
                .map(|(n, _)| n)
                .unwrap_or(size_text)
                .trim();

            if size_token.is_empty() {
                return Err("chunk size is missing".to_string());
            }

            let chunk_size =
                usize::from_str_radix(size_token, 16).map_err(|_| {
                    "chunk size is not valid hexadecimal".to_string()
                })?;

            pos = line_end + 2;

            if raw.len() < pos + chunk_size + 2 {
                return Ok(None);
            }

            out.extend_from_slice(&raw[pos..pos + chunk_size]);
            pos += chunk_size;

            if &raw[pos..pos + 2] != b"\r\n" {
                return Err(
                    "chunk data is not terminated with CRLF".to_string()
                );
            }
            pos += 2;

            if chunk_size != 0 {
                continue;
            }

            loop {
                let Some(line_end_rel) =
                    raw[pos..].windows(2).position(|w| w == b"\r\n")
                else {
                    return Ok(None);
                };

                let line_end = pos + line_end_rel;
                let line = &raw[pos..line_end];
                pos = line_end + 2;

                if line.is_empty() {
                    return Ok(Some((out, pos)));
                }
            }
        }
    }
}
