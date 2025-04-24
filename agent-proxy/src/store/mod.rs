mod binds;
pub use binds::Store as BindStore;
mod discovery;
pub use discovery::Store as DiscoveryStore;

#[derive(Clone, Debug)]
pub enum Event<T> {
	Add(T),
	Remove(T),
}
