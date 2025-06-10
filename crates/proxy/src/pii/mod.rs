mod credit_card_recognizer;
mod email_recognizer;
mod pattern_recognizer;
mod recognizer;
mod recognizer_result;
mod url_recognizer;
mod us_ssn_recognizer;
mod phone_recognizer;

#[cfg(test)]
mod tests {
	use super::*;

	use super::email_recognizer::EmailRecognizer;
	use super::url_recognizer::UrlRecognizer;
	use super::phone_recognizer::PhoneRecognizer;
	use recognizer::Recognizer;

	#[test]
	fn test_recognize() {
		let text = "Contact us at support@example.com, call (123) 456-7890, or visit https://example.com for more info. Or try info@domain.org, +1-800-555-1234, and http://another-site.org.";

		// Create recognizers
		let url_recognizer = UrlRecognizer::new();
		let email_recognizer = EmailRecognizer::new();
		let phone_recognizer = PhoneRecognizer::new();

		// Use trait objects for polymorphism
		let recognizers: Vec<&dyn Recognizer> = vec![&url_recognizer, &email_recognizer, &phone_recognizer];

		for recognizer in recognizers {
			let results = recognizer.recognize(text);
			println!("Results for {}:", recognizer.name());
			for result in results {
				println!(
					"  [{}-{}] {} (score: {})",
					result.start, result.end, result.matched, result.score
				);
			}
		}
	}
}
