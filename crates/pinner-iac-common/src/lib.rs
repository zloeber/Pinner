use std::collections::HashMap;

pub fn parse_resolve_map(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::parse_resolve_map;

    #[test]
    fn parse_entries() {
        let m = parse_resolve_map("a=b,c=d");
        assert_eq!(m.get("a").map(String::as_str), Some("b"));
        assert_eq!(m.get("c").map(String::as_str), Some("d"));
    }
}
