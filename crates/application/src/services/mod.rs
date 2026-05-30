//! Application service implementations.

mod publication;

pub use publication::{
    PublicationAcceptanceService, PublicationAcceptanceServiceDeps, PublicationAcceptanceUseCase,
};
