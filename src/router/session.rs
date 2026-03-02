use std::collections::HashMap;
use std::time::Instant;

use rand::RngCore;
use rand::rngs::OsRng;

use crate::https::Request;

use super::{SESSION_TTL, Session};

fn parse_cookie_header(cookie: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for part in cookie.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (k, v) = match trimmed.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => continue,
        };
        if !k.is_empty() {
            out.insert(k.to_string(), v.to_string());
        }
    }
    out
}

fn generate_session_id() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub(super) fn resolve_session(
    sessions: &mut HashMap<String, Session>,
    req: &Request,
    now: Instant,
) -> (Option<String>, bool) {
    let mut cookie_sid: Option<String> = None;

    if let Some(raw_cookie) = req.headers.get("cookie") {
        let cookies = parse_cookie_header(raw_cookie);
        if let Some(sid) = cookies.get("sid") {
            cookie_sid = Some(sid.clone());
        }
    }

    if let Some(sid) = cookie_sid
        && let Some(sess) = sessions.get_mut(&sid)
    {
        sess.last_seen = now;
        sess.visits = sess.visits.saturating_add(1);
        return (Some(sid), false);
    }

    let sid = generate_session_id();
    sessions.insert(
        sid.clone(),
        Session {
            id: sid.clone(),
            created_at: now,
            last_seen: now,
            visits: 1,
        },
    );

    (Some(sid), true)
}

pub(super) fn cleanup_expired_sessions(sessions: &mut HashMap<String, Session>, now: Instant) {
    sessions.retain(|_, s| now.duration_since(s.last_seen) <= SESSION_TTL);
}
