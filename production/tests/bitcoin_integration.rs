//! Bitcoin integration tests.
//!
//! Tests transaction building, fee estimation, address generation,
//! UTXO management, and Esplora API integration (mocked).

mod common;

use common::*;
use threshold_bitcoin::*;

#[tokio::test]
async fn test_transaction_builder_basic() {
    let utxos = sample_utxos();
    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22]; // Mock P2WPKH script

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let recipient = "tb1qrecipient000000000000000000000000000000".to_string();
    let result = builder
        .add_output(recipient, 10_000)
        .build_p2wpkh();

    assert!(result.is_ok(), "Basic transaction should build successfully");

    let unsigned_tx = result.unwrap();
    assert_eq!(unsigned_tx.send_amount_sats, 10_000);
    assert!(unsigned_tx.fee_sats > 0);
    assert!(unsigned_tx.sighashes.len() > 0);
}

#[tokio::test]
async fn test_transaction_with_op_return() {
    let utxos = sample_utxos();
    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let metadata = b"Hello, Bitcoin!".to_vec();
    assert!(metadata.len() <= MAX_OP_RETURN_SIZE);

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let recipient = "tb1qrecipient000000000000000000000000000000".to_string();
    let result = builder
        .add_output(recipient, 10_000)
        .add_op_return(metadata.clone());

    assert!(result.is_ok(), "OP_RETURN should be added successfully");

    let unsigned_tx = result.unwrap().build_p2wpkh().unwrap();

    // Should have payment output + OP_RETURN + change
    assert!(unsigned_tx.outputs.len() >= 2);

    // Find OP_RETURN output
    let op_return_output = unsigned_tx
        .outputs
        .iter()
        .find(|o| o.address.starts_with("OP_RETURN"));

    assert!(op_return_output.is_some(), "Should have OP_RETURN output");
    assert_eq!(op_return_output.unwrap().value, 0);
}

#[tokio::test]
async fn test_op_return_max_size() {
    let utxos = sample_utxos();
    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let metadata = vec![0u8; MAX_OP_RETURN_SIZE];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let result = builder.add_op_return(metadata);
    assert!(result.is_ok(), "Max size OP_RETURN should be accepted");
}

#[tokio::test]
async fn test_op_return_too_large() {
    let utxos = sample_utxos();
    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let metadata = vec![0u8; MAX_OP_RETURN_SIZE + 1];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let result = builder.add_op_return(metadata);
    assert!(result.is_err(), "Oversized OP_RETURN should be rejected");

    match result.unwrap_err() {
        TxBuilderError::OpReturnTooLarge { size } => {
            assert_eq!(size, MAX_OP_RETURN_SIZE + 1);
        }
        _ => panic!("Expected OpReturnTooLarge error"),
    }
}

#[tokio::test]
async fn test_multiple_outputs() {
    let utxos = sample_utxos();
    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let result = builder
        .add_output("tb1qrecipient1000000000000000000000000000000".to_string(), 10_000)
        .add_output("tb1qrecipient2000000000000000000000000000000".to_string(), 20_000)
        .add_output("tb1qrecipient3000000000000000000000000000000".to_string(), 15_000)
        .build_p2wpkh();

    assert!(result.is_ok(), "Multiple outputs should work");

    let unsigned_tx = result.unwrap();
    assert_eq!(unsigned_tx.send_amount_sats, 45_000);

    // Should have 3 payment outputs + change
    assert!(unsigned_tx.outputs.len() >= 3);
}

#[tokio::test]
async fn test_insufficient_funds() {
    let utxos = vec![Utxo {
        txid: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
        vout: 0,
        value: 1_000, // Only 1000 sats
        status: Default::default(),
    }];

    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let result = builder
        .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 10_000)
        .build_p2wpkh();

    assert!(result.is_err(), "Should fail with insufficient funds");

    match result.unwrap_err() {
        TxBuilderError::InsufficientFunds { available, required } => {
            assert_eq!(available, 1_000);
            assert!(required > available);
        }
        _ => panic!("Expected InsufficientFunds error"),
    }
}

#[tokio::test]
async fn test_no_utxos() {
    let utxos = vec![];
    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let result = builder
        .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 10_000)
        .build_p2wpkh();

    assert!(result.is_err(), "Should fail with no UTXOs");

    match result.unwrap_err() {
        TxBuilderError::NoUtxos => {
            // Expected
        }
        _ => panic!("Expected NoUtxos error"),
    }
}

#[tokio::test]
async fn test_fee_calculation() {
    let utxos = sample_utxos();
    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    // Test different fee rates
    let fee_rates = vec![1, 5, 10, 50];

    for fee_rate in fee_rates {
        let builder = TransactionBuilder::new(
            utxos.clone(),
            change_address.clone(),
            sender_script_pubkey.clone(),
            fee_rate,
        );

        let result = builder
            .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 10_000)
            .build_p2wpkh();

        assert!(result.is_ok(), "Should build with fee rate {}", fee_rate);

        let unsigned_tx = result.unwrap();
        assert!(unsigned_tx.fee_sats > 0);

        // Higher fee rate should result in higher fee
        if fee_rate > 1 {
            assert!(unsigned_tx.fee_sats >= fee_rate);
        }
    }
}

#[tokio::test]
async fn test_change_calculation() {
    let utxos = vec![Utxo {
        txid: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
        vout: 0,
        value: 100_000,
        status: Default::default(),
    }];

    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = TransactionBuilder::new(utxos, change_address.clone(), sender_script_pubkey, 5);

    let result = builder
        .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 10_000)
        .build_p2wpkh();

    assert!(result.is_ok());

    let unsigned_tx = result.unwrap();

    // Change = input - output - fee
    let expected_change = unsigned_tx.total_input_sats
        - unsigned_tx.send_amount_sats
        - unsigned_tx.fee_sats;

    assert_eq!(unsigned_tx.change_sats, expected_change);

    // Verify change output exists
    let change_output = unsigned_tx
        .outputs
        .iter()
        .find(|o| o.is_change);

    assert!(change_output.is_some());
    assert_eq!(change_output.unwrap().value, expected_change);
}

#[tokio::test]
async fn test_dust_limit() {
    let utxos = vec![Utxo {
        txid: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
        vout: 0,
        value: 100_000,
        status: Default::default(),
    }];

    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    // Send amount that would leave change below dust limit
    let result = builder
        .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 99_000)
        .build_p2wpkh();

    assert!(result.is_ok());

    let unsigned_tx = result.unwrap();

    // If change is below dust limit, it should be absorbed into fee
    if unsigned_tx.change_sats < DUST_LIMIT {
        let change_output = unsigned_tx.outputs.iter().find(|o| o.is_change);
        assert!(change_output.is_none(), "Dust change should not create output");
    }
}

#[tokio::test]
async fn test_taproot_transaction_building() {
    let utxos = sample_utxos();
    let change_address = "tb1pchange00000000000000000000000000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 34]; // Mock P2TR script

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let recipient = "tb1precipient0000000000000000000000000000000000000000000000000".to_string();
    let result = builder
        .add_output(recipient, 10_000)
        .build_p2tr();

    assert!(result.is_ok(), "Taproot transaction should build successfully");

    let unsigned_tx = result.unwrap();
    assert_eq!(unsigned_tx.send_amount_sats, 10_000);
    assert!(unsigned_tx.sighashes.len() > 0);
}

#[tokio::test]
async fn test_sighash_generation_p2wpkh() {
    let utxos = sample_utxos();
    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = TransactionBuilder::new(utxos.clone(), change_address, sender_script_pubkey, 5);

    let result = builder
        .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 10_000)
        .build_p2wpkh();

    assert!(result.is_ok());

    let unsigned_tx = result.unwrap();

    // Should have one sighash per input
    assert_eq!(unsigned_tx.sighashes.len(), unsigned_tx.inputs.len());

    // Each sighash should be 32 bytes (hex = 64 chars)
    for sighash in &unsigned_tx.sighashes {
        assert_eq!(sighash.len(), 64, "Sighash should be 32 bytes in hex");
    }
}

#[tokio::test]
async fn test_utxo_selection_greedy() {
    let utxos = vec![
        Utxo {
            txid: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            vout: 0,
            value: 10_000,
            status: Default::default(),
        },
        Utxo {
            txid: "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
            vout: 0,
            value: 100_000, // Largest
            status: Default::default(),
        },
        Utxo {
            txid: "0000000000000000000000000000000000000000000000000000000000000003".to_string(),
            vout: 0,
            value: 50_000,
            status: Default::default(),
        },
    ];

    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let result = builder
        .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 20_000)
        .build_p2wpkh();

    assert!(result.is_ok());

    let unsigned_tx = result.unwrap();

    // Greedy algorithm should select largest UTXO first (100k)
    // which is sufficient for 20k payment
    assert!(unsigned_tx.inputs.len() >= 1);

    // First input should be the largest UTXO
    assert!(unsigned_tx.inputs.iter().any(|i| i.value == 100_000));
}

#[tokio::test]
async fn test_finalize_p2wpkh_transaction() {
    let utxos = vec![Utxo {
        txid: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
        vout: 0,
        value: 100_000,
        status: Default::default(),
    }];

    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let unsigned_tx = builder
        .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 10_000)
        .build_p2wpkh()
        .unwrap();

    // Mock signatures (one per input)
    let signatures: Vec<Vec<u8>> = unsigned_tx
        .inputs
        .iter()
        .map(|_| MockCrypto::mock_ecdsa_signature())
        .collect();

    let public_keys: Vec<Vec<u8>> = unsigned_tx
        .inputs
        .iter()
        .map(|_| MockCrypto::mock_compressed_pubkey())
        .collect();

    let result = finalize_p2wpkh_transaction(
        &unsigned_tx.unsigned_tx_hex,
        &signatures,
        &public_keys,
    );

    assert!(result.is_ok(), "Should finalize transaction");

    let signed_tx_hex = result.unwrap();
    assert!(signed_tx_hex.len() > unsigned_tx.unsigned_tx_hex.len());
}

#[tokio::test]
async fn test_finalize_taproot_transaction() {
    let utxos = vec![Utxo {
        txid: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
        vout: 0,
        value: 100_000,
        status: Default::default(),
    }];

    let change_address = "tb1pchange00000000000000000000000000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 34];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let unsigned_tx = builder
        .add_output("tb1precipient0000000000000000000000000000000000000000000000000".to_string(), 10_000)
        .build_p2tr()
        .unwrap();

    // Mock Schnorr signatures (64 bytes each)
    let signatures: Vec<Vec<u8>> = unsigned_tx
        .inputs
        .iter()
        .map(|_| MockCrypto::mock_schnorr_signature())
        .collect();

    let result = finalize_taproot_transaction(&unsigned_tx.unsigned_tx_hex, &signatures);

    assert!(result.is_ok(), "Should finalize Taproot transaction");
}

#[tokio::test]
async fn test_finalize_wrong_signature_count() {
    let utxos = vec![
        Utxo {
            txid: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            vout: 0,
            value: 50_000,
            status: Default::default(),
        },
        Utxo {
            txid: "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
            vout: 0,
            value: 50_000,
            status: Default::default(),
        },
    ];

    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let unsigned_tx = builder
        .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 10_000)
        .build_p2wpkh()
        .unwrap();

    // Provide wrong number of signatures (only 1 instead of 2)
    let signatures = vec![MockCrypto::mock_ecdsa_signature()];
    let public_keys = vec![MockCrypto::mock_compressed_pubkey()];

    let result = finalize_p2wpkh_transaction(
        &unsigned_tx.unsigned_tx_hex,
        &signatures,
        &public_keys,
    );

    assert!(result.is_err(), "Should fail with wrong signature count");

    match result.unwrap_err() {
        TxBuilderError::InvalidSignatureLength { .. } => {
            // Expected
        }
        _ => panic!("Expected InvalidSignatureLength error"),
    }
}

#[tokio::test]
async fn test_complex_transaction_scenario() {
    let utxos = sample_utxos();
    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let metadata = b"MPC Wallet v1.0 - Multisig Transaction".to_vec();

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 10);

    let result = builder
        .add_output("tb1qrecipient1000000000000000000000000000000".to_string(), 25_000)
        .add_output("tb1qrecipient2000000000000000000000000000000".to_string(), 30_000)
        .add_op_return(metadata)
        .unwrap()
        .build_p2wpkh();

    assert!(result.is_ok());

    let unsigned_tx = result.unwrap();

    assert_eq!(unsigned_tx.send_amount_sats, 55_000);
    assert!(unsigned_tx.fee_sats > 0);
    assert!(unsigned_tx.outputs.len() >= 3); // 2 payments + OP_RETURN + maybe change

    // Verify OP_RETURN output
    let op_return = unsigned_tx.outputs.iter().find(|o| o.address.starts_with("OP_RETURN"));
    assert!(op_return.is_some());
}

#[tokio::test]
async fn test_address_validation() {
    let utxos = sample_utxos();
    let change_address = "invalid_address".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = TransactionBuilder::new(utxos, change_address, sender_script_pubkey, 5);

    let result = builder
        .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 10_000)
        .build_p2wpkh();

    assert!(result.is_err(), "Should fail with invalid change address");

    match result.unwrap_err() {
        TxBuilderError::InvalidChangeAddress(_) => {
            // Expected
        }
        _ => panic!("Expected InvalidChangeAddress error"),
    }
}
