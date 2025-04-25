use agent_proxy::Config;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::{cmp, env};

pub fn parse_config() -> anyhow::Result<Config> {
	Ok(agent_proxy::Config {
		network: parse("NETWORK")?.unwrap_or_default(),
		backend_mesh: parse_default("BACKEND_MESH", true)?,

		self_termination_deadline: match parse_duration("CONNECTION_TERMINATION_DEADLINE")? {
			Some(period) => period,
			None => match parse::<u64>("TERMINATION_GRACE_PERIOD_SECONDS")? {
				// We want our drain period to be less than Kubernetes, so we can use the last few seconds
				// to abruptly terminate anything remaining before Kubernetes SIGKILLs us.
				// We could just take the SIGKILL, but it is even more abrupt (TCP RST vs RST_STREAM/TLS close, etc)
				// Note: we do this in code instead of in configuration so that we can use downward API to expose this variable
				// if it is added to Kubernetes (https://github.com/kubernetes/kubernetes/pull/125746).
				Some(secs) => Duration::from_secs(cmp::max(
					if secs > 10 {
						secs - 5
					} else {
						// If the grace period is really low give less buffer
						secs - 1
					},
					1,
				)),
				None => Duration::from_secs(5),
			},
		},
		hbone: Arc::new(agent_hbone::Config {
			// window size: per-stream limit
			window_size: parse_default("HTTP2_STREAM_WINDOW_SIZE", 4u32 * 1024 * 1024)?,
			// connection window size: per connection.
			// Setting this to the same value as window_size can introduce deadlocks in some applications
			// where clients do not read data on streamA until they receive data on streamB.
			// If streamA consumes the entire connection window, we enter a deadlock.
			// A 4x limit should be appropriate without introducing too much potential buffering.
			connection_window_size: parse_default("HTTP2_CONNECTION_WINDOW_SIZE", 16u32 * 1024 * 1024)?,
			frame_size: parse_default("HTTP2_FRAME_SIZE", 1024u32 * 1024)?,

			pool_max_streams_per_conn: parse_default("POOL_MAX_STREAMS_PER_CONNECTION", 100u16)?,

			pool_unused_release_timeout: parse_duration_default(
				"POOL_UNUSED_RELEASE_TIMEOUT",
				Duration::from_secs(60 * 5),
			)?,
		}),
	})
}

fn parse<T: FromStr>(env: &str) -> anyhow::Result<Option<T>>
where
	<T as FromStr>::Err: ToString,
{
	match env::var(env) {
		Ok(val) => val
			.parse()
			.map(|v| Some(v))
			.map_err(|e: <T as FromStr>::Err| {
				anyhow::anyhow!("invalid env var {}={} ({})", env, val, e.to_string())
			}),
		Err(_) => Ok(None),
	}
}

fn parse_default<T: FromStr>(env: &str, default: T) -> anyhow::Result<T>
where
	<T as FromStr>::Err: std::error::Error + Sync + Send,
{
	parse(env).map(|v| v.unwrap_or(default))
}

fn parse_duration(env: &str) -> anyhow::Result<Option<Duration>> {
	parse::<String>(env)?
		.map(|ds| {
			duration_str::parse(&ds)
				.map_err(|e| anyhow::anyhow!("invalid env var {}={} ({})", env, ds, e))
		})
		.transpose()
}
fn parse_duration_default(env: &str, default: Duration) -> anyhow::Result<Duration> {
	parse_duration(env).map(|v| v.unwrap_or(default))
}
