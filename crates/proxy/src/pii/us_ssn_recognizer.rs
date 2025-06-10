use regex::Regex;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Pattern {
    pub name: &'static str,
    pub regex: Regex,
    pub score: f32,
}

pub struct UsSsnRecognizer {
    patterns: Vec<Pattern>,
    context: Vec<&'static str>,
}

impl UsSsnRecognizer {
    pub fn new() -> Self {
        let patterns = vec![
            Pattern {
                name: "SSN1 (very weak)",
                regex: Regex::new(r"\b([0-9]{5})-([0-9]{4})\b").unwrap(),
                score: 0.05,
            },
            Pattern {
                name: "SSN2 (very weak)",
                regex: Regex::new(r"\b([0-9]{3})-([0-9]{6})\b").unwrap(),
                score: 0.05,
            },
            Pattern {
                name: "SSN3 (very weak)",
                regex: Regex::new(r"\b(([0-9]{3})-([0-9]{2})-([0-9]{4}))\b").unwrap(),
                score: 0.05,
            },
            Pattern {
                name: "SSN4 (very weak)",
                regex: Regex::new(r"\b[0-9]{9}\b").unwrap(),
                score: 0.05,
            },
            Pattern {
                name: "SSN5 (medium)",
                regex: Regex::new(r"\b([0-9]{3})[- .]([0-9]{2})[- .]([0-9]{4})\b").unwrap(),
                score: 0.5,
            },
        ];
        let context = vec![
            "social", "security", "ssn", "ssns", "ssid",
        ];
        Self { patterns, context }
    }

    pub fn recognize(&self, text: &str) -> Vec<String> {
        let mut results = Vec::new();
        for pattern in &self.patterns {
            for cap in pattern.regex.captures_iter(text) {
                if let Some(matched) = cap.get(0) {
                    let candidate = matched.as_str();
                    if !Self::invalidate_result(candidate) {
                        results.push(candidate.to_string());
                    }
                }
            }
        }
        results
    }

    fn invalidate_result(pattern_text: &str) -> bool {
        // Count delimiters
        let mut delimiter_counts: HashMap<char, usize> = HashMap::new();
        for c in pattern_text.chars() {
            if c == '.' || c == '-' || c == ' ' {
                *delimiter_counts.entry(c).or_insert(0) += 1;
            }
        }
        if delimiter_counts.keys().len() > 1 {
            return true;
        }
        // Only digits
        let only_digits: String = pattern_text.chars().filter(|c| c.is_ascii_digit()).collect();
        if only_digits.chars().all(|c| c == only_digits.chars().next().unwrap_or(' ')) {
            return true;
        }
        if only_digits.len() >= 9 {
            if &only_digits[3..5] == "00" || &only_digits[5..] == "0000" {
                return true;
            }
        }
        for sample_ssn in ["000", "666", "123456789", "98765432", "078051120"] {
            if only_digits.starts_with(sample_ssn) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_invalidate_result() {
        assert!(UsSsnRecognizer::invalidate_result("111-11-1111")); // all same digit
        assert!(UsSsnRecognizer::invalidate_result("000-12-3456")); // starts with 000
        assert!(UsSsnRecognizer::invalidate_result("123-45-6789")); // starts with 123456789
        assert!(!UsSsnRecognizer::invalidate_result("123-45-6788")); // valid
    }
    #[test]
    fn test_recognize() {
        let recognizer = UsSsnRecognizer::new();
        let text = "My SSN is 123-45-6788 and should be detected.";
        let results = recognizer.recognize(text);
        assert_eq!(results, vec!["123-45-6788"]);
    }
} 