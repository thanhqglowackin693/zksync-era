#![feature(generic_const_exprs)]
use std::fs::File;
use std::io::Read;

use circuit_definitions::boojum::cs::implementations::hints::{
    DenseVariablesCopyHint, DenseWitnessCopyHint,
};
use circuit_definitions::boojum::cs::implementations::polynomial_storage::{
    SetupBaseStorage, SetupStorage,
};
use circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver;
use circuit_definitions::boojum::cs::implementations::verifier::VerificationKey;
use circuit_definitions::boojum::cs::oracle::merkle_tree::MerkleTreeWithCap;
use circuit_definitions::boojum::cs::oracle::TreeHasher;
use circuit_definitions::boojum::field::{PrimeField, SmallField};

use circuit_definitions::boojum::field::traits::field_like::PrimeFieldLikeVectorized;

use circuit_definitions::circuit_definitions::base_layer::ZkSyncBaseLayerVerificationKey;
use circuit_definitions::circuit_definitions::recursion_layer::{
    ZkSyncRecursionLayerStorageType, ZkSyncRecursionLayerVerificationKey,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zksync_config::configs::FriProverConfig;
use zksync_types::proofs::AggregationRound;

pub mod in_memory_setup_data_source;
pub mod utils;

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    bound = "F: serde::Serialize + serde::de::DeserializeOwned, P: serde::Serialize + serde::de::DeserializeOwned"
)]
pub struct ProverSetupData<
    F: PrimeField + SmallField,
    P: PrimeFieldLikeVectorized<Base = F>,
    H: TreeHasher<F>,
> {
    pub setup_base: SetupBaseStorage<F, P>,
    pub setup: SetupStorage<F, P>,
    #[serde(bound(
        serialize = "H::Output: serde::Serialize",
        deserialize = "H::Output: serde::de::DeserializeOwned"
    ))]
    pub vk: VerificationKey<F, H>,
    #[serde(bound(
        serialize = "H::Output: serde::Serialize",
        deserialize = "H::Output: serde::de::DeserializeOwned"
    ))]
    pub setup_tree: MerkleTreeWithCap<F, H>,
    pub vars_hint: DenseVariablesCopyHint,
    pub wits_hint: DenseWitnessCopyHint,
    pub finalization_hint: FinalizationHintsForProver,
}

enum ProverServiceDataType {
    VerificationKey,
    SetupData,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ProverServiceDataKey {
    pub circuit_id: u8,
    pub round: AggregationRound,
}

impl ProverServiceDataKey {
    pub fn new(circuit_id: u8, round: AggregationRound) -> Self {
        Self { circuit_id, round }
    }
}

pub fn get_base_path() -> String {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| "/".into());
    format!(
        "{}/core/bin/vk_setup_data_generator_server_fri/data",
        zksync_home
    )
}

pub fn get_base_vk_path() -> String {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| "/".into());
    format!(
        "{}/core/bin/vk_setup_data_generator_server_fri/data",
        zksync_home
    )
}

fn get_file_path(key: ProverServiceDataKey, service_data_type: ProverServiceDataType) -> String {
    let name = match key.round {
        AggregationRound::BasicCircuits => {
            format!("basic_{}", key.circuit_id)
        }
        AggregationRound::LeafAggregation => {
            format!("leaf_{}", key.circuit_id)
        }
        AggregationRound::NodeAggregation => "node".to_string(),
        AggregationRound::Scheduler => "scheduler".to_string(),
    };
    match service_data_type {
        ProverServiceDataType::VerificationKey => {
            format!("{}/verification_{}_key.json", get_base_vk_path(), name)
        }
        ProverServiceDataType::SetupData => {
            format!(
                "{}/setup_{}_data.bin",
                FriProverConfig::from_env().setup_data_path,
                name
            )
        }
    }
}

pub fn get_base_layer_vk_for_circuit_type(circuit_type: u8) -> ZkSyncBaseLayerVerificationKey {
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits),
        ProverServiceDataType::VerificationKey,
    );
    vlog::info!("Fetching verification key from path: {}", filepath);
    let text = std::fs::read_to_string(&filepath)
        .unwrap_or_else(|_| panic!("Failed reading verification key from path: {}", filepath));
    serde_json::from_str::<ZkSyncBaseLayerVerificationKey>(&text).unwrap_or_else(|_| {
        panic!(
            "Failed deserializing verification key from path: {}",
            filepath
        )
    })
}

pub fn get_recursive_layer_vk_for_circuit_type(
    circuit_type: u8,
) -> ZkSyncRecursionLayerVerificationKey {
    let round = get_round_for_recursive_circuit_type(circuit_type);
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, round),
        ProverServiceDataType::VerificationKey,
    );
    vlog::info!("Fetching verification key from path: {}", filepath);
    let text = std::fs::read_to_string(&filepath)
        .unwrap_or_else(|_| panic!("Failed reading verification key from path: {}", filepath));
    serde_json::from_str::<ZkSyncRecursionLayerVerificationKey>(&text).unwrap_or_else(|_| {
        panic!(
            "Failed deserializing verification key from path: {}",
            filepath
        )
    })
}

pub fn get_round_for_recursive_circuit_type(circuit_type: u8) -> AggregationRound {
    match circuit_type {
        circuit_type if circuit_type == ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8 => {
            AggregationRound::Scheduler
        }
        circuit_type if circuit_type == ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8 => {
            AggregationRound::NodeAggregation
        }
        _ => AggregationRound::LeafAggregation,
    }
}

pub fn save_base_layer_vk(vk: ZkSyncBaseLayerVerificationKey) {
    let circuit_type = vk.numeric_circuit_type();
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits),
        ProverServiceDataType::VerificationKey,
    );
    vlog::info!("saving basic verification key to: {}", filepath);
    std::fs::write(filepath, serde_json::to_string_pretty(&vk).unwrap()).unwrap();
}

pub fn save_recursive_layer_vk(vk: ZkSyncRecursionLayerVerificationKey) {
    let circuit_type = vk.numeric_circuit_type();
    let round = get_round_for_recursive_circuit_type(circuit_type);
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, round),
        ProverServiceDataType::VerificationKey,
    );
    vlog::info!("saving recursive layer verification key to: {}", filepath);
    std::fs::write(filepath, serde_json::to_string_pretty(&vk).unwrap()).unwrap();
}

pub fn get_setup_data_for_circuit_type<F, P, H>(
    key: ProverServiceDataKey,
) -> ProverSetupData<F, P, H>
where
    F: PrimeField + SmallField + Serialize + DeserializeOwned,
    P: PrimeFieldLikeVectorized<Base = F> + Serialize + DeserializeOwned,
    H: TreeHasher<F>,
    <H as TreeHasher<F>>::Output: Serialize + DeserializeOwned,
{
    let filepath = get_file_path(key.clone(), ProverServiceDataType::SetupData);
    let mut file = File::open(filepath.clone())
        .unwrap_or_else(|_| panic!("Failed reading setup-data from path: {:?}", filepath));
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap_or_else(|_| {
        panic!(
            "Failed reading setup-data to buffer from path: {:?}",
            filepath
        )
    });
    vlog::info!("loading {:?} setup data from path: {}", key, filepath);
    bincode::deserialize::<ProverSetupData<F, P, H>>(&buffer).unwrap_or_else(|_| {
        panic!(
            "Failed deserializing setup-data at path: {:?} for circuit: {:?}",
            filepath, key
        )
    })
}

pub fn save_setup_data(key: ProverServiceDataKey, serialized_setup_data: &Vec<u8>) {
    let filepath = get_file_path(key.clone(), ProverServiceDataType::SetupData);
    vlog::info!("saving {:?} setup data to: {}", key, filepath);
    std::fs::write(filepath.clone(), serialized_setup_data)
        .unwrap_or_else(|_| panic!("Failed saving setup-data at path: {:?}", filepath));
}
