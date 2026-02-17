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
    ReadingBody {
        header_end: usize,
        content_length: usize,
    },
    Responding,
}
impl Conn {
    pub fn read_outcome(&mut self, new_bytes: &[u8]) -> ReadOutcome {
        self.in_buf.extend_from_slice(new_bytes);

        match self.state {
            ConnState::ReadingHeaders => self.read_headers(),
            ConnState::ReadingBody {
                header_end,
                content_length,
            } => self.read_body(header_end, content_length),
            ConnState::Responding => ReadOutcome::Pending,
        }
    }

    fn read_headers(&mut self) -> ReadOutcome {
        let Some(header_end) = self.find_header_end() else {
            return ReadOutcome::Pending;
        };

        let content_length = match Self::parse_content_length(&self.in_buf[..header_end]) {
            Ok(v) => v,
            Err(reason) => {
                return ReadOutcome::Error {
                    status: StatusCode::BadRequest,
                    reason,
                };
            }
        };

        if content_length == 0 {
            return ReadOutcome::Ready(self.build_pending_request(header_end, 0));
        }

        self.state = ConnState::ReadingBody {
            header_end,
            content_length,
        };
        self.read_body(header_end, content_length)
    }

    fn read_body(&mut self, header_end: usize, content_length: usize) -> ReadOutcome {
        let total_len = header_end + content_length;
        if self.in_buf.len() < total_len {
            return ReadOutcome::Pending;
        }

        ReadOutcome::Ready(self.build_pending_request(header_end, content_length))
    }

    fn build_pending_request(
        &mut self,
        header_end: usize,
        content_length: usize,
    ) -> PendingRequest {
        let total_len = header_end + content_length;
        PendingRequest {
            header_bytes: self.in_buf[..header_end].to_vec(),
            body_bytes: self.in_buf[header_end..total_len].to_vec(),
            local_port: self.local_port,
        }
    }

    fn find_header_end(&mut self) -> Option<usize> {
        self.in_buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| i + 4)
    }

    fn parse_content_length(header_bytes: &[u8]) -> Result<usize, String> {
        let text = std::str::from_utf8(header_bytes)
            .map_err(|_| "request headers are not valid UTF-8".to_string())?;
        let mut lines = text.split("\r\n");

        let _ = lines
            .next()
            .ok_or_else(|| "missing request line".to_string())?;

        let mut content_length: Option<usize> = None;

        for line in lines {
            if line.is_empty() {
                break;
            }

            let Some((name, value)) = line.split_once(':') else {
                continue;
            };

            if !name.eq_ignore_ascii_case("Content-Length") {
                continue;
            }

            if content_length.is_some() {
                return Err("duplicate Content-Length header".to_string());
            }

            let parsed = value
                .trim()
                .parse::<usize>()
                .map_err(|_| "Content-Length must be a positive integer".to_string())?;
            content_length = Some(parsed);
        }

        Ok(content_length.unwrap_or(0))
    }
}
