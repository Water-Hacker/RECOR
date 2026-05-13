pub mod kafka_consumer;
pub mod mock_bunec;
pub mod outbox_admin;
pub mod postgres;
pub mod relay;
pub mod retention;

pub use kafka_consumer::{ConsumeOutcome, KafkaConsumer, ParseResult};
pub use mock_bunec::PostgresMockBunec;
pub use outbox_admin::{DlqRow, OutboxAdminError, OutboxAdminStore};
pub use postgres::PostgresVerificationRepository;
pub use relay::{VerificationOutboxRelay, WritebackSubscriber};
pub use retention::{PruneOutcome, VerificationOutboxRetention};
