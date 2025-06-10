use regex::Regex;
use super::recognizer_result::RecognizerResult;
use super::recognizer::Recognizer;

#[derive(Debug)]
pub struct Pattern {
    pub name: String,
    pub regex: Regex,
    pub score: f32,
}

pub trait PatternValidator {
    fn validate(&self, pattern_text: &str) -> Option<bool> {
        None
    }
    fn invalidate(&self, pattern_text: &str) -> Option<bool> {
        None
    }
}

pub struct PatternRecognizer<'a> {
    patterns: Vec<Pattern>,
    context: Vec<String>,
    entity_type: String,
    validator: Option<&'a dyn PatternValidator>,
}

impl<'a> PatternRecognizer<'a> {
    pub fn new(entity_type: &str, context: Vec<String>) -> Self {
        Self {
            patterns: Vec::new(),
            context,
            entity_type: entity_type.to_string(),
            validator: None,
        }
    }

    pub fn with_validator(mut self, validator: &'a dyn PatternValidator) -> Self {
        self.validator = Some(validator);
        self
    }

    pub fn add_pattern(&mut self, name: &str, regex: &str, score: f32) {
        let pattern = Pattern {
            name: name.to_string(),
            regex: Regex::new(regex).unwrap(),
            score,
        };
        self.patterns.push(pattern);
    }
}

impl<'a> Recognizer for PatternRecognizer<'a> {
    fn recognize(&self, text: &str) -> Vec<RecognizerResult> {
        let mut results = Vec::new();
        for pattern in &self.patterns {
            for cap in pattern.regex.captures_iter(text) {
                if let Some(matched) = cap.get(0) {
                    let candidate = matched.as_str();
                    let mut score = pattern.score;
                    let mut valid = true;
                    if let Some(validator) = self.validator {
                        if let Some(false) = validator.validate(candidate) {
                            valid = false;
                            score = 0.0;
                        }
                        if let Some(true) = validator.invalidate(candidate) {
                            valid = false;
                            score = 0.0;
                        }
                    }
                    if valid {
                        results.push(RecognizerResult {
                            entity_type: self.entity_type.clone(),
                            matched: candidate.to_string(),
                            start: matched.start(),
                            end: matched.end(),
                            score,
                        });
                    }
                }
            }
        }
        results
    }
    fn name(&self) -> &str {
        &self.entity_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::recognizer::Recognizer;
    struct DummyValidator;
    impl PatternValidator for DummyValidator {
        fn validate(&self, pattern_text: &str) -> Option<bool> {
            if pattern_text == "forbidden" { Some(false) } else { None }
        }
        fn invalidate(&self, pattern_text: &str) -> Option<bool> {
            if pattern_text == "bad" { Some(true) } else { None }
        }
    }
    #[test]
    fn test_pattern_recognizer() {
        let mut recognizer = PatternRecognizer::new("TEST", vec!["test".to_string()]);
        recognizer.add_pattern("test", r"\btest\b", 1.0);
        let results = recognizer.recognize("this is a test string");
        assert_eq!(results, vec![RecognizerResult {
            entity_type: "TEST".to_string(),
            matched: "test".to_string(),
            start: 10,
            end: 14,
            score: 1.0,
        }]);
    }
    #[test]
    fn test_pattern_recognizer_with_validator() {
        let mut recognizer = PatternRecognizer::new("TEST", vec![]).with_validator(&DummyValidator);
        recognizer.add_pattern("forbidden", r"forbidden", 1.0);
        recognizer.add_pattern("bad", r"bad", 1.0);
        let results = recognizer.recognize("good forbidden bad");
        assert_eq!(results, Vec::<RecognizerResult>::new());
    }
} 