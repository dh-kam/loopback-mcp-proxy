#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buffer: String,
}

impl SseParser {
    pub fn push(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();
        loop {
            let Some(index) = find_separator(&self.buffer) else {
                break;
            };
            let (raw, size) = index;
            let block = self.buffer[..raw].to_string();
            self.buffer.drain(..size);
            if let Some(event) = parse_block(&block) {
                events.push(event);
            }
        }
        if self.buffer.len() > 8 * 1024 * 1024 {
            self.buffer.clear();
        }
        events
    }
}

fn find_separator(buffer: &str) -> Option<(usize, usize)> {
    if let Some(index) = buffer.find("\n\n") {
        return Some((index, index + 2));
    }
    if let Some(index) = buffer.find("\r\n\r\n") {
        return Some((index, index + 4));
    }
    None
}

fn parse_block(block: &str) -> Option<SseEvent> {
    let mut event = String::new();
    let mut data: Vec<String> = Vec::new();
    for line in block.split(['\n', '\r']) {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            event = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        }
    }
    if data.is_empty() && event.is_empty() {
        return None;
    }
    Some(SseEvent {
        event,
        data: data.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_events_across_chunks() {
        let mut parser = SseParser::default();
        assert!(parser.push("event: endpoint\ndata: /mess").is_empty());
        let events = parser.push("ages/1\n\n: ping\n\ndata: {\"id\":1}\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "endpoint");
        assert_eq!(events[0].data, "/messages/1");
        assert_eq!(events[1].data, "{\"id\":1}");
    }
}
