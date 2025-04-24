use http::{HeaderName, HeaderValue, Request};
use minijinja::Environment;
use std::collections::HashMap;

pub struct Transformation {
	env: Environment<'static>,
	request_headers: HashMap<HeaderName, String>,
}

fn build(request_headers: HashMap<HeaderName, String>) -> anyhow::Result<Transformation> {
	let mut env: Environment<'static> = Environment::new();
	let mut res = HashMap::new();
	for (k, t) in request_headers.into_iter() {
		let name = format!("request_header_{}", k);
		env.add_template_owned(name.clone(), t)?;
		// }
		res.insert(k, name);
	}
	Ok(Transformation { env, request_headers: res })
}

impl Transformation {
	pub fn apply(&self, req: &mut Request<()>) {
		for (name, tmpl_key) in self.request_headers.iter() {
			let tmpl = self
				.env
				.get_template(tmpl_key)
				.expect("template must exist");
			let res = tmpl.render(());
			req.headers_mut().insert(
				name,
				HeaderValue::try_from(res.unwrap_or_else(|_| "template render failed".to_string())).unwrap(),
			);
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::http::transformation::Transformation;
	use std::collections::HashMap;
	use http::HeaderName;

	fn build<const N: usize>(items: [(&str, &str); N]) -> Transformation {
		let hm = items.iter().map(|(k, v)| {
			(HeaderName::try_from(*k).unwrap(), v.to_string())
		}).collect();
		super::build(hm).unwrap()
	}
	
	#[test]
	fn test_transformation() {
		let mut req = ::http::Request::builder()
			.method("GET")
			.uri("https://www.rust-lang.org/")
			.header("X-Custom-Foo", "Bar")
			.body(())
			.unwrap();
		let xfm = build([("x-insert", r#"{{ "hello world" }}}"#)]);
		xfm.apply(&mut req);
		panic!("{req:?}")
	}
}
