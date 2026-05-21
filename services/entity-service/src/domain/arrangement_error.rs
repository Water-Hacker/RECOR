//! Domain errors raised by the `Arrangement` aggregate.

use thiserror::Error;

use super::arrangement_value_object::ArrangementValueObjectError;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArrangementDomainError {
    #[error("arrangement {0} already registered; duplicate registration rejected")]
    AlreadyRegistered(uuid::Uuid),

    #[error("arrangement {0} not found")]
    NotFound(uuid::Uuid),

    #[error("arrangement {0} is dissolved; further updates are refused")]
    UpdateOnDissolved(uuid::Uuid),

    #[error("arrangement {0} has no prior registration; update refused")]
    UpdateBeforeRegistration(uuid::Uuid),

    #[error("arrangement {0} has no prior registration; dissolution refused")]
    DissolveBeforeRegistration(uuid::Uuid),

    #[error("arrangement {arrangement_id} is already dissolved on {dissolution_date}")]
    AlreadyDissolved {
        arrangement_id: uuid::Uuid,
        dissolution_date: time::Date,
    },

    #[error("constitution_date {constitution_date} is in the future (now {now}); FATF R.25 INR §3 refuses arrangements that have not yet been constituted")]
    ConstitutionDateInFuture {
        constitution_date: time::Date,
        now: time::Date,
    },

    #[error("dissolution_date {dissolution_date} must be strictly after constitution_date {constitution_date}")]
    DissolutionBeforeOrEqualConstitution {
        constitution_date: time::Date,
        dissolution_date: time::Date,
    },

    #[error("FATF R.25 INR §3.a refuses an arrangement with no settlor")]
    NoSettlor,

    #[error("FATF R.25 INR §3.b refuses an arrangement with no trustee")]
    NoTrustee,

    /// Retention computation overflowed the `Date` representable range
    /// (R.25 INR §3.f mandates 5-year-after-cessation retention, so a
    /// dissolution date close to the upper Date bound could overflow).
    #[error("retention_until overflowed Date range for dissolution_date {0}")]
    RetentionUntilOverflow(time::Date),

    #[error(transparent)]
    ValueObject(#[from] ArrangementValueObjectError),
}
