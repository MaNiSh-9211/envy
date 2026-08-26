pub const KNOWN_SCHEMES: &[&str] = &[
    "postgresql",
    "postgres",
    "mysql",
    "mariadb",
    "mssql",
    "mongodb",
    "mongodb+srv",
    "redis",
    "rediss",
    "amqp",
    "rabbitmq",
    "kafka",
    "http",
    "https",
    "ws",
    "wss",
    "grpc",
    "ftp",
    "sftp",
    "ssh",
    "smtp",
    "s3",
    "gs",
    "azblob",
    "sqlite",
];

const BOOLEAN_WORDS: &[&str] = &["true", "false", "1", "0", "yes", "no", "on", "off"];

pub fn levenshtein(a: &str, b: &str) -> usize {
    osa_distance(a, b)
}

/// Optimal string alignment (Damerau-Levenshtein without adjacent transposition
/// repeats) — catches typos like `ture` → `true` that plain Levenshtein overcounts.
fn osa_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1
                && j > 1
                && a[i - 1] == b[j - 2]
                && a[i - 2] == b[j - 1]
            {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

pub fn best_match<'a, I>(input: &str, candidates: I) -> Option<String>
where
    I: IntoIterator<Item = &'a String>,
{
    let mut best: Option<(usize, &String)> = None;
    for candidate in candidates {
        if candidate == input {
            continue;
        }
        let distance = levenshtein(input, candidate);
        let limit = (input.len() / 3).clamp(1, 3);
        if distance <= limit && best.map_or(true, |(bd, _)| distance < bd) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, name)| name.clone())
}

pub fn scheme_hint(raw: &str) -> Option<String> {
    let (scheme, rest) = raw.split_once("://")?;
    if scheme.is_empty() || KNOWN_SCHEMES.contains(&scheme) {
        return None;
    }
    let suggestion = best_match(scheme, KNOWN_SCHEMES.iter().map(|s| s.to_string()).collect::<Vec<_>>().iter())?;
    Some(format!("{suggestion}://{rest}"))
}

pub fn boolean_hint(raw: &str) -> Option<&'static str> {
    let lowered = raw.to_ascii_lowercase();
    best_match(&lowered, BOOLEAN_WORDS.iter().map(|s| s.to_string()).collect::<Vec<_>>().iter())
        .and_then(|hit| BOOLEAN_WORDS.iter().find(|w| **w == hit).copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn distance_basics() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("", "ab"), 2);
    }

    #[test]
    fn finds_nearby_keys() {
        let schema = candidates(&["DATABASE_URL", "PORT", "API_SECRET"]);
        assert_eq!(best_match("DATABASEURL", &schema).as_deref(), Some("DATABASE_URL"));
        assert_eq!(best_match("TOTALLY_DIFFERENT", &schema), None);
    }

    #[test]
    fn fixes_scheme_typos() {
        assert_eq!(
            scheme_hint("postgersql://user@db/xyz"),
            Some("postgresql://user@db/xyz".to_string())
        );
        assert_eq!(scheme_hint("postgresql://fine"), None);
    }

    #[test]
    fn fixes_boolean_typos() {
        assert_eq!(boolean_hint("ture"), Some("true"));
        assert_eq!(boolean_hint("validword"), None);
    }
}
