// Copyright Istio Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fmt::{Debug, Display};
use std::sync::{Arc, RwLock};

use agent_core::prelude::*;

// Do not warn is it is WIP
#[allow(unused, dead_code)]
mod ext_proc;

pub mod gateway;
pub mod hbone;
mod http;
mod store;
pub mod stream;
pub mod transport;
pub mod types;
pub mod util;

use serde::Serializer;

pub fn is_default<T: Default + PartialEq>(t: &T) -> bool {
	*t == Default::default()
}

fn serialize_option_display<S: Serializer, T: Display>(
	t: &Option<T>,
	serializer: S,
) -> Result<S::Ok, S::Error> {
	match t {
		None => serializer.serialize_none(),
		Some(t) => serializer.serialize_str(&t.to_string()),
	}
}

pub struct Config {
	pub network: Strng,
	backend_mesh: bool,
	self_termination_deadline: Duration,
	hbone: Arc<agent_hbone::Config>,
}

#[derive(Clone, Debug)]
pub struct ConfigStore {
	binds: Arc<RwLock<store::BindStore>>,
	workloads: Arc<RwLock<store::DiscoveryStore>>,
}

impl ConfigStore {
	pub fn read_binds(&self) -> std::sync::RwLockReadGuard<'_, store::BindStore> {
		self.binds.read().expect("mutex acquired")
	}

	pub fn read_discovery(&self) -> std::sync::RwLockReadGuard<'_, store::DiscoveryStore> {
		self.workloads.read().expect("mutex acquired")
	}
}

#[derive(Clone)]
pub struct Metrics {}

#[derive(Clone)]
pub struct ProxyInputs {
	cfg: Arc<Config>,
	store: ConfigStore,
	_metrics: Arc<Metrics>,
	local_workload_information: Arc<hbone::LocalWorkloadInformation>,
}

#[allow(clippy::too_many_arguments)]
impl ProxyInputs {
	pub fn new(
		cfg: Arc<Config>,
		store: ConfigStore,
		metrics: Arc<Metrics>,
		local_workload_information: Arc<hbone::LocalWorkloadInformation>,
	) -> Arc<Self> {
		Arc::new(Self {
			cfg,
			store,
			_metrics: metrics,
			local_workload_information,
		})
	}
}
