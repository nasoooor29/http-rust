use std::collections::HashMap;

pub(super) fn match_pattern(pattern: &str, req_path: &str) -> Option<HashMap<String, String>> {
    let p = pattern.trim_matches('/');
    let r = req_path.trim_matches('/');

    let p_segs: Vec<&str> = if p.is_empty() {
        vec![]
    } else {
        p.split('/').collect()
    };
    let r_segs: Vec<&str> = if r.is_empty() {
        vec![]
    } else {
        r.split('/').collect()
    };

    if p_segs.len() != r_segs.len() {
        return None;
    }

    let mut out = HashMap::new();

    for (ps, rs) in p_segs.iter().zip(r_segs.iter()) {
        if let Some(name) = ps.strip_prefix(':') {
            if name.is_empty() {
                return None;
            }
            out.insert(name.to_string(), (*rs).to_string());
            continue;
        }

        if ps != rs {
            return None;
        }
    }

    Some(out)
}

pub(super) fn parse_query(query: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if query.is_empty() {
        return out;
    }

    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        if !k.is_empty() {
            out.insert(k.to_string(), v.to_string());
        }
    }

    out
}
