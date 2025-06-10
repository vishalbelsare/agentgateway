use super::pattern_recognizer::PatternRecognizer;
use super::recognizer::Recognizer;

pub struct EmailRecognizer<'a> {
    recognizer: PatternRecognizer<'a>,
}

impl<'a> EmailRecognizer<'a> {
    pub fn new() -> Self {
        let mut recognizer = PatternRecognizer::new(
            "EMAIL_ADDRESS",
            vec!["email".to_string(), "e-mail".to_string(), "mail".to_string()],
        );
        // Standard email regex (simplified, but robust for most cases)
        let email_regex = r"[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+";
        recognizer.add_pattern("Standard Email", email_regex, 0.85);
        Self { recognizer }
    }
}

impl<'a> Recognizer for EmailRecognizer<'a> {
    fn recognize(&self, text: &str) -> Vec<super::recognizer_result::RecognizerResult> {
        self.recognizer.recognize(text)
    }
    fn name(&self) -> &str {
        self.recognizer.name()
    }
} 