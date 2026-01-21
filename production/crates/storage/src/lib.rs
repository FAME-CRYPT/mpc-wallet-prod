pub mod etcd;
pub mod postgres;

pub use etcd::EtcdStorage;
pub use postgres::PostgresStorage;
