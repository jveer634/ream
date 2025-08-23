use actix_web::web::ServiceConfig;

use crate::handlers::{
    duties::{get_attester_duties, get_proposer_duties},
    prepare_beacon_proposer::prepare_beacon_proposer,
    validator::{get_attestation_data, post_contribution_and_proofs},
};

pub fn register_validator_routes(config: &mut ServiceConfig) {
    config
        .service(get_proposer_duties)
        .service(get_attester_duties)
        .service(prepare_beacon_proposer)
        .service(get_attestation_data)
        .service(post_contribution_and_proofs);
}
