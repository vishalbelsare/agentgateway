use regex::Regex;

#[derive(Debug)]
pub struct Pattern {
    pub name: &'static str,
    pub regex: Regex,
    pub score: f32,
}

pub struct CreditCardRecognizer {
    patterns: Vec<Pattern>,
    context: Vec<&'static str>,
    replacement_pairs: Vec<(&'static str, &'static str)>,
}

impl CreditCardRecognizer {
    pub fn new() -> Self {
        let pattern = Pattern {
            name: "All Credit Cards (weak)",
            regex: Regex::new(r"\b(?!1\d{12}(?!\d))((4\d{3})|(5[0-5]\d{2})|(6\d{3})|(1\d{3})|(3\d{3}))[- ]?(\d{3,4})[- ]?(\d{3,4})[- ]?(\d{3,5})\b").unwrap(),
            score: 0.3,
        };
        let context = vec![
            "credit", "card", "visa", "mastercard", "cc ", "amex", "discover", "jcb", "diners", "maestro", "instapayment",
        ];
        let replacement_pairs = vec![("-", ""), (" ", "")];
        Self {
            patterns: vec![pattern],
            context,
            replacement_pairs,
        }
    }

    pub fn recognize(&self, text: &str) -> Vec<String> {
        let mut results = Vec::new();
        for pattern in &self.patterns {
            for cap in pattern.regex.captures_iter(text) {
                if let Some(matched) = cap.get(0) {
                    let candidate = matched.as_str();
                    if self.validate_result(candidate) {
                        results.push(candidate.to_string());
                    }
                }
            }
        }
        results
    }

    fn sanitize_value(&self, value: &str) -> String {
        let mut sanitized = value.to_string();
        for (from, to) in &self.replacement_pairs {
            sanitized = sanitized.replace(from, to);
        }
        sanitized
    }

    fn validate_result(&self, pattern_text: &str) -> bool {
        let sanitized_value = self.sanitize_value(pattern_text);
        Self::luhn_checksum(&sanitized_value)
    }

    fn luhn_checksum(sanitized_value: &str) -> bool {
        let digits: Vec<u32> = sanitized_value.chars().filter_map(|c| c.to_digit(10)).collect();
        let mut sum = 0;
        let len = digits.len();
        for (i, digit) in digits.iter().rev().enumerate() {
            let mut n = *digit;
            if i % 2 == 1 {
                n *= 2;
                if n > 9 {
                    n -= 9;
                }
            }
            sum += n;
        }
        sum % 10 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_luhn_checksum() {
        assert!(CreditCardRecognizer::luhn_checksum("4532015112830366")); // valid Visa
        assert!(!CreditCardRecognizer::luhn_checksum("4532015112830367")); // invalid
    }
    #[test]
    fn test_recognize() {
        let recognizer = CreditCardRecognizer::new();
        let text = "My card is 4532-0151-1283-0366 and should be detected.";
        let results = recognizer.recognize(text);
        assert_eq!(results, vec!["4532-0151-1283-0366"]);
    }
} 