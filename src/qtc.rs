use bincode::{deserialize, serialize};
use bs58;
use fin_plan_program::FinPlanState;
use fin_plan_transaction::FinPlanTransaction;
use chrono::prelude::*;
use clap::ArgMatches;
use blockthread::NodeInfo;
use faucet::DroneRequest;
use fullnode::Config;
use hash::Hash;
use reqwest;
use reqwest::header::CONTENT_TYPE;
use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use serde_json::{self, Value};
use signature::{Keypair, KeypairUtil, Signature};
use xpz_program_interface::pubkey::Pubkey;
use std::fs::{self, File};
use std::io::prelude::*;
use std::io::{Error, ErrorKind, Write};
use std::mem::size_of;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use std::{error, fmt, mem};
use builtin_tansaction::SystemTransaction;
use transaction::Transaction;

#[derive(Debug, PartialEq)]
pub enum QtcCommand {
    Address,
    AirDrop(i64),
    Balance,
    Cancel(Pubkey),
    Confirm(Signature),
    Pay(
        i64,
        Pubkey,
        Option<DateTime<Utc>>,
        Option<Pubkey>,
        Option<Vec<Pubkey>>,
        Option<Pubkey>,
    ),

    TimeElapsed(Pubkey, Pubkey, DateTime<Utc>),

    Witness(Pubkey, Pubkey),
}

#[derive(Debug, Clone)]
pub enum QtcError {
    CommandNotRecognized(String),
    BadParameter(String),
    RpcRequestError(String),
}

impl fmt::Display for QtcError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "invalid")
    }
}

impl error::Error for QtcError {
    fn description(&self) -> &str {
        "invalid"
    }

    fn cause(&self) -> Option<&error::Error> {

        None
    }
}

pub struct QtcConfig {
    pub leader: NodeInfo,
    pub id: Keypair,
    pub faucet_addr: SocketAddr,
    pub rpc_addr: String,
    pub command: QtcCommand,
}

impl Default for QtcConfig {
    fn default() -> QtcConfig {
        let default_addr = socketaddr!(0, 8000);
        QtcConfig {
            leader: NodeInfo::new_with_socketaddr(&default_addr),
            id: Keypair::new(),
            faucet_addr: default_addr,
            rpc_addr: default_addr.to_string(),
            command: QtcCommand::Balance,
        }
    }
}

pub fn parse_command(
    pubkey: Pubkey,
    matches: &ArgMatches,
) -> Result<QtcCommand, Box<error::Error>> {
    let response = match matches.subcommand() {
        ("address", Some(_address_matches)) => Ok(QtcCommand::Address),
        ("airdrop", Some(airdrop_matches)) => {
            let tokens = airdrop_matches.value_of("tokens").unwrap().parse()?;
            Ok(QtcCommand::AirDrop(tokens))
        }
        ("balance", Some(_balance_matches)) => Ok(QtcCommand::Balance),
        ("cancel", Some(cancel_matches)) => {
            let pubkey_vec = bs58::decode(cancel_matches.value_of("process-id").unwrap())
                .into_vec()
                .expect("base58-encoded public key");

            if pubkey_vec.len() != mem::size_of::<Pubkey>() {
                eprintln!("{}", cancel_matches.usage());
                Err(QtcError::BadParameter("Invalid public key".to_string()))?;
            }
            let process_id = Pubkey::new(&pubkey_vec);
            Ok(QtcCommand::Cancel(process_id))
        }
        ("confirm", Some(confirm_matches)) => {
            let signatures = bs58::decode(confirm_matches.value_of("signature").unwrap())
                .into_vec()
                .expect("base58-encoded signature");

            if signatures.len() == mem::size_of::<Signature>() {
                let signature = Signature::new(&signatures);
                Ok(QtcCommand::Confirm(signature))
            } else {
                eprintln!("{}", confirm_matches.usage());
                Err(QtcError::BadParameter("Invalid signature".to_string()))
            }
        }
        ("pay", Some(pay_matches)) => {
            let tokens = pay_matches.value_of("tokens").unwrap().parse()?;
            let to = if pay_matches.is_present("to") {
                let pubkey_vec = bs58::decode(pay_matches.value_of("to").unwrap())
                    .into_vec()
                    .expect("base58-encoded public key");

                if pubkey_vec.len() != mem::size_of::<Pubkey>() {
                    eprintln!("{}", pay_matches.usage());
                    Err(QtcError::BadParameter(
                        "Invalid to public key".to_string(),
                    ))?;
                }
                Pubkey::new(&pubkey_vec)
            } else {
                pubkey
            };
            let timestamp = if pay_matches.is_present("timestamp") {

                let date_string = if !pay_matches.value_of("timestamp").unwrap().contains('Z') {
                    format!("\"{}Z\"", pay_matches.value_of("timestamp").unwrap())
                } else {
                    format!("\"{}\"", pay_matches.value_of("timestamp").unwrap())
                };
                Some(serde_json::from_str(&date_string)?)
            } else {
                None
            };
            let timestamp_pubkey = if pay_matches.is_present("timestamp-pubkey") {
                let pubkey_vec = bs58::decode(pay_matches.value_of("timestamp-pubkey").unwrap())
                    .into_vec()
                    .expect("base58-encoded public key");

                if pubkey_vec.len() != mem::size_of::<Pubkey>() {
                    eprintln!("{}", pay_matches.usage());
                    Err(QtcError::BadParameter(
                        "Invalid timestamp public key".to_string(),
                    ))?;
                }
                Some(Pubkey::new(&pubkey_vec))
            } else {
                None
            };
            let witness_vec = if pay_matches.is_present("witness") {
                let witnesses = pay_matches.values_of("witness").unwrap();
                let mut collection = Vec::new();
                for witness in witnesses {
                    let pubkey_vec = bs58::decode(witness)
                        .into_vec()
                        .expect("base58-encoded public key");

                    if pubkey_vec.len() != mem::size_of::<Pubkey>() {
                        eprintln!("{}", pay_matches.usage());
                        Err(QtcError::BadParameter(
                            "Invalid witness public key".to_string(),
                        ))?;
                    }
                    collection.push(Pubkey::new(&pubkey_vec));
                }
                Some(collection)
            } else {
                None
            };
            let cancelable = if pay_matches.is_present("cancelable") {
                Some(pubkey)
            } else {
                None
            };

            Ok(QtcCommand::Pay(
                tokens,
                to,
                timestamp,
                timestamp_pubkey,
                witness_vec,
                cancelable,
            ))
        }
        ("send-signature", Some(sig_matches)) => {
            let pubkey_vec = bs58::decode(sig_matches.value_of("to").unwrap())
                .into_vec()
                .expect("base58-encoded public key");

            if pubkey_vec.len() != mem::size_of::<Pubkey>() {
                eprintln!("{}", sig_matches.usage());
                Err(QtcError::BadParameter("Invalid public key".to_string()))?;
            }
            let to = Pubkey::new(&pubkey_vec);

            let pubkey_vec = bs58::decode(sig_matches.value_of("process-id").unwrap())
                .into_vec()
                .expect("base58-encoded public key");

            if pubkey_vec.len() != mem::size_of::<Pubkey>() {
                eprintln!("{}", sig_matches.usage());
                Err(QtcError::BadParameter("Invalid public key".to_string()))?;
            }
            let process_id = Pubkey::new(&pubkey_vec);
            Ok(QtcCommand::Witness(to, process_id))
        }
        ("send-timestamp", Some(timestamp_matches)) => {
            let pubkey_vec = bs58::decode(timestamp_matches.value_of("to").unwrap())
                .into_vec()
                .expect("base58-encoded public key");

            if pubkey_vec.len() != mem::size_of::<Pubkey>() {
                eprintln!("{}", timestamp_matches.usage());
                Err(QtcError::BadParameter("Invalid public key".to_string()))?;
            }
            let to = Pubkey::new(&pubkey_vec);

            let pubkey_vec = bs58::decode(timestamp_matches.value_of("process-id").unwrap())
                .into_vec()
                .expect("base58-encoded public key");

            if pubkey_vec.len() != mem::size_of::<Pubkey>() {
                eprintln!("{}", timestamp_matches.usage());
                Err(QtcError::BadParameter("Invalid public key".to_string()))?;
            }
            let process_id = Pubkey::new(&pubkey_vec);
            let dt = if timestamp_matches.is_present("datetime") {
                // Parse input for serde_json
                let date_string = if !timestamp_matches
                    .value_of("datetime")
                    .unwrap()
                    .contains('Z')
                {
                    format!("\"{}Z\"", timestamp_matches.value_of("datetime").unwrap())
                } else {
                    format!("\"{}\"", timestamp_matches.value_of("datetime").unwrap())
                };
                serde_json::from_str(&date_string)?
            } else {
                Utc::now()
            };
            Ok(QtcCommand::TimeElapsed(to, process_id, dt))
        }
        ("", None) => {
            eprintln!("{}", matches.usage());
            Err(QtcError::CommandNotRecognized(
                "no subcommand given".to_string(),
            ))
        }
        _ => unreachable!(),
    }?;
    Ok(response)
}

pub fn process_command(config: &QtcConfig) -> Result<String, Box<error::Error>> {
    match config.command {

        QtcCommand::Address => Ok(format!("{}", config.id.pubkey())),

        QtcCommand::AirDrop(tokens) => {
            println!(
                "Requesting airdrop of {:?} tokens from {}",
                tokens, config.faucet_addr
            );
            let params = json!(format!("{}", config.id.pubkey()));
            let previous_balance = match QtcRpcRequest::GetBalance
                .make_rpc_request(&config.rpc_addr, 1, Some(params))?
                .as_i64()
            {
                Some(tokens) => tokens,
                None => Err(QtcError::RpcRequestError(
                    "Received result of an unexpected type".to_string(),
                ))?,
            };
            request_airdrop(&config.faucet_addr, &config.id.pubkey(), tokens as u64)?;


            let mut current_balance = previous_balance;
            for _ in 0..20 {
                sleep(Duration::from_millis(500));
                let params = json!(format!("{}", config.id.pubkey()));
                current_balance = QtcRpcRequest::GetBalance
                    .make_rpc_request(&config.rpc_addr, 1, Some(params))?
                    .as_i64()
                    .unwrap_or(previous_balance);

                if previous_balance != current_balance {
                    break;
                }
                println!(".");
            }
            if current_balance - previous_balance != tokens {
                Err("Airdrop failed!")?;
            }
            Ok(format!("Your balance is: {:?}", current_balance))
        }

        QtcCommand::Balance => {
            println!("Balance requested...");
            let params = json!(format!("{}", config.id.pubkey()));
            let balance = QtcRpcRequest::GetBalance
                .make_rpc_request(&config.rpc_addr, 1, Some(params))?
                .as_i64();
            match balance {
                Some(0) => Ok("No account found! Request an airdrop to get started.".to_string()),
                Some(tokens) => Ok(format!("Your balance is: {:?}", tokens)),
                None => Err(QtcError::RpcRequestError(
                    "Received result of an unexpected type".to_string(),
                ))?,
            }
        }

        QtcCommand::Cancel(pubkey) => {
            let last_id = get_last_id(&config)?;

            let tx =
                Transaction::fin_plan_new_signature(&config.id, pubkey, config.id.pubkey(), last_id);
            let signature_str = serialize_and_send_tx(&config, &tx)?;

            Ok(signature_str.to_string())
        }

        QtcCommand::Confirm(signature) => {
            let params = json!(format!("{}", signature));
            let confirmation = QtcRpcRequest::ConfirmTransaction
                .make_rpc_request(&config.rpc_addr, 1, Some(params))?
                .as_bool();
            match confirmation {
                Some(b) => {
                    if b {
                        Ok("Confirmed".to_string())
                    } else {
                        Ok("Not found".to_string())
                    }
                }
                None => Err(QtcError::RpcRequestError(
                    "Received result of an unexpected type".to_string(),
                ))?,
            }
        }

        QtcCommand::Pay(tokens, to, timestamp, timestamp_pubkey, ref witnesses, cancelable) => {
            let last_id = get_last_id(&config)?;

            if timestamp == None && *witnesses == None {
                let tx = Transaction::system_new(&config.id, to, tokens, last_id);
                let signature_str = serialize_and_send_tx(&config, &tx)?;
                Ok(signature_str.to_string())
            } else if *witnesses == None {
                let dt = timestamp.unwrap();
                let dt_pubkey = match timestamp_pubkey {
                    Some(pubkey) => pubkey,
                    None => config.id.pubkey(),
                };

                let contract_funds = Keypair::new();
                let contract_state = Keypair::new();
                let fin_plan_program_id = FinPlanState::id();


                let tx = Transaction::system_create(
                    &config.id,
                    contract_funds.pubkey(),
                    last_id,
                    tokens,
                    0,
                    fin_plan_program_id,
                    0,
                );
                let _signature_str = serialize_and_send_tx(&config, &tx)?;


                let tx = Transaction::system_create(
                    &config.id,
                    contract_state.pubkey(),
                    last_id,
                    1,
                    196,
                    fin_plan_program_id,
                    0,
                );
                let _signature_str = serialize_and_send_tx(&config, &tx)?;


                let tx = Transaction::fin_plan_new_on_date(
                    &contract_funds,
                    to,
                    contract_state.pubkey(),
                    dt,
                    dt_pubkey,
                    cancelable,
                    tokens,
                    last_id,
                );
                let signature_str = serialize_and_send_tx(&config, &tx)?;

                Ok(json!({
                    "signature": signature_str,
                    "processId": format!("{}", contract_state.pubkey()),
                }).to_string())
            } else if timestamp == None {
                let last_id = get_last_id(&config)?;

                let witness = if let Some(ref witness_vec) = *witnesses {
                    witness_vec[0]
                } else {
                    Err(QtcError::BadParameter(
                        "Could not parse required signature pubkey(s)".to_string(),
                    ))?
                };

                let contract_funds = Keypair::new();
                let contract_state = Keypair::new();
                let fin_plan_program_id = FinPlanState::id();

  
                let tx = Transaction::system_create(
                    &config.id,
                    contract_funds.pubkey(),
                    last_id,
                    tokens,
                    0,
                    fin_plan_program_id,
                    0,
                );
                let _signature_str = serialize_and_send_tx(&config, &tx)?;

                let tx = Transaction::system_create(
                    &config.id,
                    contract_state.pubkey(),
                    last_id,
                    1,
                    196,
                    fin_plan_program_id,
                    0,
                );
                let _signature_str = serialize_and_send_tx(&config, &tx)?;


                let tx = Transaction::fin_plan_new_when_signed(
                    &contract_funds,
                    to,
                    contract_state.pubkey(),
                    witness,
                    cancelable,
                    tokens,
                    last_id,
                );
                let signature_str = serialize_and_send_tx(&config, &tx)?;

                Ok(json!({
                    "signature": signature_str,
                    "processId": format!("{}", contract_state.pubkey()),
                }).to_string())
            } else {
                Ok("Combo transactions not yet handled".to_string())
            }
        }

        QtcCommand::TimeElapsed(to, pubkey, dt) => {
            let params = json!(format!("{}", config.id.pubkey()));
            let balance = QtcRpcRequest::GetBalance
                .make_rpc_request(&config.rpc_addr, 1, Some(params))?
                .as_i64();
            if let Some(0) = balance {
                request_airdrop(&config.faucet_addr, &config.id.pubkey(), 1)?;
            }

            let last_id = get_last_id(&config)?;

            let tx = Transaction::fin_plan_new_timestamp(&config.id, pubkey, to, dt, last_id);
            let signature_str = serialize_and_send_tx(&config, &tx)?;

            Ok(signature_str.to_string())
        }

        QtcCommand::Witness(to, pubkey) => {
            let last_id = get_last_id(&config)?;

            let params = json!(format!("{}", config.id.pubkey()));
            let balance = QtcRpcRequest::GetBalance
                .make_rpc_request(&config.rpc_addr, 1, Some(params))?
                .as_i64();
            if let Some(0) = balance {
                request_airdrop(&config.faucet_addr, &config.id.pubkey(), 1)?;
            }

            let tx = Transaction::fin_plan_new_signature(&config.id, pubkey, to, last_id);
            let signature_str = serialize_and_send_tx(&config, &tx)?;

            Ok(signature_str.to_string())
        }
    }
}

pub fn read_leader(path: &str) -> Result<Config, QtcError> {
    let file = File::open(path.to_string()).or_else(|err| {
        Err(QtcError::BadParameter(format!(
            "{}: Unable to open leader file: {}",
            err, path
        )))
    })?;

    serde_json::from_reader(file).or_else(|err| {
        Err(QtcError::BadParameter(format!(
            "{}: Failed to parse leader file: {}",
            err, path
        )))
    })
}

pub fn request_airdrop(
    faucet_addr: &SocketAddr,
    id: &Pubkey,
    tokens: u64,
) -> Result<Signature, Error> {
    // TODO: make this async tokio client
    let mut stream = TcpStream::connect(faucet_addr)?;
    let req = DroneRequest::GetAirdrop {
        airdrop_request_amount: tokens,
        client_pubkey: *id,
    };
    let tx = serialize(&req).expect("serialize faucet request");
    stream.write_all(&tx)?;
    let mut buffer = [0; size_of::<Signature>()];
    stream
        .read_exact(&mut buffer)
        .or_else(|_| Err(Error::new(ErrorKind::Other, "Airdrop failed")))?;
    let signature: Signature = deserialize(&buffer).or_else(|err| {
        Err(Error::new(
            ErrorKind::Other,
            format!("deserialize signature in request_airdrop: {:?}", err),
        ))
    })?;
    // TODO: add timeout to this function, in case of unresponsive faucet
    Ok(signature)
}

pub fn gen_keypair_file(outfile: String) -> Result<String, Box<error::Error>> {
    let rnd = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rnd)?;
    let serialized = serde_json::to_string(&pkcs8_bytes.to_vec())?;

    if outfile != "-" {
        if let Some(outdir) = Path::new(&outfile).parent() {
            fs::create_dir_all(outdir)?;
        }
        let mut f = File::create(outfile)?;
        f.write_all(&serialized.clone().into_bytes())?;
    }
    Ok(serialized)
}

pub enum QtcRpcRequest {
    ConfirmTransaction,
    GetAccountInfo,
    GetBalance,
    GetFinality,
    GetLastId,
    GetTransactionCount,
    RequestAirdrop,
    SendTransaction,
}
impl QtcRpcRequest {
    fn make_rpc_request(
        &self,
        rpc_addr: &str,
        id: u64,
        params: Option<Value>,
    ) -> Result<Value, Box<error::Error>> {
        let jsonrpc = "2.0";
        let method = match self {
            QtcRpcRequest::ConfirmTransaction => "confirmTransaction",
            QtcRpcRequest::GetAccountInfo => "getAccountInfo",
            QtcRpcRequest::GetBalance => "getBalance",
            QtcRpcRequest::GetFinality => "getFinality",
            QtcRpcRequest::GetLastId => "getLastId",
            QtcRpcRequest::GetTransactionCount => "getTransactionCount",
            QtcRpcRequest::RequestAirdrop => "requestAirdrop",
            QtcRpcRequest::SendTransaction => "sendTransaction",
        };
        let client = reqwest::Client::new();
        let mut request = json!({
           "jsonrpc": jsonrpc,
           "id": id,
           "method": method,
        });
        if let Some(param_string) = params {
            request["params"] = json!(vec![param_string]);
        }
        let mut response = client
            .post(rpc_addr)
            .header(CONTENT_TYPE, "application/json")
            .body(request.to_string())
            .send()?;
        let json: Value = serde_json::from_str(&response.text()?)?;
        if json["error"].is_object() {
            Err(QtcError::RpcRequestError(format!(
                "RPC Error response: {}",
                serde_json::to_string(&json["error"]).unwrap()
            )))?
        }
        Ok(json["result"].clone())
    }
}

fn get_last_id(config: &QtcConfig) -> Result<Hash, Box<error::Error>> {
    let result = QtcRpcRequest::GetLastId.make_rpc_request(&config.rpc_addr, 1, None)?;
    if result.as_str().is_none() {
        Err(QtcError::RpcRequestError(
            "Received bad last_id".to_string(),
        ))?
    }
    let last_id_str = result.as_str().unwrap();
    let last_id_vec = bs58::decode(last_id_str)
        .into_vec()
        .map_err(|_| QtcError::RpcRequestError("Received bad last_id".to_string()))?;
    Ok(Hash::new(&last_id_vec))
}

fn serialize_and_send_tx(
    config: &QtcConfig,
    tx: &Transaction,
) -> Result<String, Box<error::Error>> {
    let serialized = serialize(tx).unwrap();
    let params = json!(serialized);
    let signature =
        QtcRpcRequest::SendTransaction.make_rpc_request(&config.rpc_addr, 2, Some(params))?;
    if signature.as_str().is_none() {
        Err(QtcError::RpcRequestError(
            "Received result of an unexpected type".to_string(),
        ))?
    }
    Ok(signature.as_str().unwrap().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use transaction_processor::TransactionProcessor;
    use clap::{App, Arg, SubCommand};
    use blockthread::Node;
    use faucet::run_local_faucet;
    use fullnode::Fullnode;
    use ledger::LedgerWriter;
    use mint::Mint;
    use signature::{read_keypair, read_pkcs8, Keypair, KeypairUtil};
    use std::fs::remove_dir_all;
    use std::sync::mpsc::channel;

    fn tmp_ledger(name: &str, mint: &Mint) -> String {
        use std::env;
        let out_dir = env::var("OUT_DIR").unwrap_or_else(|_| "target".to_string());
        let keypair = Keypair::new();

        let path = format!("{}/tmp-ledger-{}-{}", out_dir, name, keypair.pubkey());

        let mut writer = LedgerWriter::open(&path, true).unwrap();
        writer.write_entries(mint.create_entries()).unwrap();

        path
    }

    #[test]
    fn test_qtc_parse_command() {
        let test_commands = App::new("test")
            .subcommand(SubCommand::with_name("address").about("Get your public key"))
            .subcommand(
                SubCommand::with_name("airdrop")
                    .about("Request a batch of tokens")
                    .arg(
                        Arg::with_name("tokens")
                            .index(1)
                            .value_name("NUM")
                            .takes_value(true)
                            .required(true)
                            .help("The number of tokens to request"),
                    ),
            ).subcommand(SubCommand::with_name("balance").about("Get your balance"))
            .subcommand(
                SubCommand::with_name("cancel")
                    .about("Cancel a transfer")
                    .arg(
                        Arg::with_name("process-id")
                            .index(1)
                            .value_name("PROCESS_ID")
                            .takes_value(true)
                            .required(true)
                            .help("The process id of the transfer to cancel"),
                    ),
            ).subcommand(
                SubCommand::with_name("confirm")
                    .about("Confirm transaction by signature")
                    .arg(
                        Arg::with_name("signature")
                            .index(1)
                            .value_name("SIGNATURE")
                            .takes_value(true)
                            .required(true)
                            .help("The transaction signature to confirm"),
                    ),
            ).subcommand(
                SubCommand::with_name("pay")
                    .about("Send a payment")
                    .arg(
                        Arg::with_name("to")
                            .index(1)
                            .value_name("PUBKEY")
                            .takes_value(true)
                            .required(true)
                            .help("The pubkey of recipient"),
                    ).arg(
                        Arg::with_name("tokens")
                            .index(2)
                            .value_name("NUM")
                            .takes_value(true)
                            .required(true)
                            .help("The number of tokens to send"),
                    ).arg(
                        Arg::with_name("timestamp")
                            .long("after")
                            .value_name("DATETIME")
                            .takes_value(true)
                            .help("A timestamp after which transaction will execute"),
                    ).arg(
                        Arg::with_name("timestamp-pubkey")
                            .long("require-timestamp-from")
                            .value_name("PUBKEY")
                            .takes_value(true)
                            .requires("timestamp")
                            .help("Require timestamp from this third party"),
                    ).arg(
                        Arg::with_name("witness")
                            .long("require-signature-from")
                            .value_name("PUBKEY")
                            .takes_value(true)
                            .multiple(true)
                            .use_delimiter(true)
                            .help("Any third party signatures required to unlock the tokens"),
                    ).arg(
                        Arg::with_name("cancelable")
                            .long("cancelable")
                            .takes_value(false),
                    ),
            ).subcommand(
                SubCommand::with_name("send-signature")
                    .about("Send a signature to authorize a transfer")
                    .arg(
                        Arg::with_name("to")
                            .index(1)
                            .value_name("PUBKEY")
                            .takes_value(true)
                            .required(true)
                            .help("The pubkey of recipient"),
                    ).arg(
                        Arg::with_name("process-id")
                            .index(2)
                            .value_name("PROCESS_ID")
                            .takes_value(true)
                            .required(true)
                            .help("The process id of the transfer to authorize"),
                    ),
            ).subcommand(
                SubCommand::with_name("send-timestamp")
                    .about("Send a timestamp to unlock a transfer")
                    .arg(
                        Arg::with_name("to")
                            .index(1)
                            .value_name("PUBKEY")
                            .takes_value(true)
                            .required(true)
                            .help("The pubkey of recipient"),
                    ).arg(
                        Arg::with_name("process-id")
                            .index(2)
                            .value_name("PROCESS_ID")
                            .takes_value(true)
                            .required(true)
                            .help("The process id of the transfer to unlock"),
                    ).arg(
                        Arg::with_name("datetime")
                            .long("date")
                            .value_name("DATETIME")
                            .takes_value(true)
                            .help("Optional arbitrary timestamp to apply"),
                    ),
            );
        let pubkey = Keypair::new().pubkey();
        let pubkey_string = format!("{}", pubkey);
        let witness0 = Keypair::new().pubkey();
        let witness0_string = format!("{}", witness0);
        let witness1 = Keypair::new().pubkey();
        let witness1_string = format!("{}", witness1);
        let dt = Utc.ymd(2018, 9, 19).and_hms(17, 30, 59);


        let test_airdrop = test_commands
            .clone()
            .get_matches_from(vec!["test", "airdrop", "50"]);
        assert_eq!(
            parse_command(pubkey, &test_airdrop).unwrap(),
            QtcCommand::AirDrop(50)
        );
        let test_bad_airdrop = test_commands
            .clone()
            .get_matches_from(vec!["test", "airdrop", "notint"]);
        assert!(parse_command(pubkey, &test_bad_airdrop).is_err());

        let test_cancel =
            test_commands
                .clone()
                .get_matches_from(vec!["test", "cancel", &pubkey_string]);
        assert_eq!(
            parse_command(pubkey, &test_cancel).unwrap(),
            QtcCommand::Cancel(pubkey)
        );



        let signature = Signature::new(&vec![1; 64]);
        let signature_string = format!("{:?}", signature);
        let test_confirm =
            test_commands
                .clone()
                .get_matches_from(vec!["test", "confirm", &signature_string]);
        assert_eq!(
            parse_command(pubkey, &test_confirm).unwrap(),
            QtcCommand::Confirm(signature)
        );
        let test_bad_signature = test_commands
            .clone()
            .get_matches_from(vec!["test", "confirm", "deadbeef"]);
        assert!(parse_command(pubkey, &test_bad_signature).is_err());

        let test_pay =
            test_commands
                .clone()
                .get_matches_from(vec!["test", "pay", &pubkey_string, "50"]);
        assert_eq!(
            parse_command(pubkey, &test_pay).unwrap(),
            QtcCommand::Pay(50, pubkey, None, None, None, None)
        );
        let test_bad_pubkey = test_commands
            .clone()
            .get_matches_from(vec!["test", "pay", "deadbeef", "50"]);
        assert!(parse_command(pubkey, &test_bad_pubkey).is_err());


        let test_pay_multiple_witnesses = test_commands.clone().get_matches_from(vec![
            "test",
            "pay",
            &pubkey_string,
            "50",
            "--require-signature-from",
            &witness0_string,
            "--require-signature-from",
            &witness1_string,
        ]);
        assert_eq!(
            parse_command(pubkey, &test_pay_multiple_witnesses).unwrap(),
            QtcCommand::Pay(50, pubkey, None, None, Some(vec![witness0, witness1]), None)
        );
        let test_pay_single_witness = test_commands.clone().get_matches_from(vec![
            "test",
            "pay",
            &pubkey_string,
            "50",
            "--require-signature-from",
            &witness0_string,
        ]);
        assert_eq!(
            parse_command(pubkey, &test_pay_single_witness).unwrap(),
            QtcCommand::Pay(50, pubkey, None, None, Some(vec![witness0]), None)
        );


        let test_pay_timestamp = test_commands.clone().get_matches_from(vec![
            "test",
            "pay",
            &pubkey_string,
            "50",
            "--after",
            "2018-09-19T17:30:59",
            "--require-timestamp-from",
            &witness0_string,
        ]);
        assert_eq!(
            parse_command(pubkey, &test_pay_timestamp).unwrap(),
            QtcCommand::Pay(50, pubkey, Some(dt), Some(witness0), None, None)
        );


        let test_send_signature = test_commands.clone().get_matches_from(vec![
            "test",
            "send-signature",
            &pubkey_string,
            &pubkey_string,
        ]);
        assert_eq!(
            parse_command(pubkey, &test_send_signature).unwrap(),
            QtcCommand::Witness(pubkey, pubkey)
        );
        let test_pay_multiple_witnesses = test_commands.clone().get_matches_from(vec![
            "test",
            "pay",
            &pubkey_string,
            "50",
            "--after",
            "2018-09-19T17:30:59",
            "--require-signature-from",
            &witness0_string,
            "--require-timestamp-from",
            &witness0_string,
            "--require-signature-from",
            &witness1_string,
        ]);
        assert_eq!(
            parse_command(pubkey, &test_pay_multiple_witnesses).unwrap(),
            QtcCommand::Pay(
                50,
                pubkey,
                Some(dt),
                Some(witness0),
                Some(vec![witness0, witness1]),
                None
            )
        );


        let test_send_timestamp = test_commands.clone().get_matches_from(vec![
            "test",
            "send-timestamp",
            &pubkey_string,
            &pubkey_string,
            "--date",
            "2018-09-19T17:30:59",
        ]);
        assert_eq!(
            parse_command(pubkey, &test_send_timestamp).unwrap(),
            QtcCommand::TimeElapsed(pubkey, pubkey, dt)
        );
        let test_bad_timestamp = test_commands.clone().get_matches_from(vec![
            "test",
            "send-timestamp",
            &pubkey_string,
            &pubkey_string,
            "--date",
            "20180919T17:30:59",
        ]);
        assert!(parse_command(pubkey, &test_bad_timestamp).is_err());
    }
    #[test]
    #[ignore]
    fn test_qtc_process_command() {
        let leader_keypair = Keypair::new();
        let leader = Node::new_localhost_with_pubkey(leader_keypair.pubkey());

        let alice = Mint::new(10_000_000);
        let transaction_processor = TransactionProcessor::new(&alice);
        let bob_pubkey = Keypair::new().pubkey();
        let leader_data = leader.info.clone();
        let leader_data1 = leader.info.clone();
        let ledger_path = tmp_ledger("qtc_process_command", &alice);

        let mut config = QtcConfig::default();
        let rpc_port = 12345; // Needs to be distinct known number to not conflict with other tests

        let server = Fullnode::new_with_transaction_processor(
            leader_keypair,
            transaction_processor,
            0,
            &[],
            leader,
            None,
            &ledger_path,
            false,
            None,
            Some(rpc_port),
        );
        sleep(Duration::from_millis(900));

        let (sender, receiver) = channel();
        run_local_faucet(alice.keypair(), leader_data.contact_info.ncp, sender);
        config.faucet_addr = receiver.recv().unwrap();
        config.leader = leader_data1;

        let mut rpc_addr = leader_data.contact_info.ncp;
        rpc_addr.set_port(rpc_port);
        config.rpc_addr = format!("http://{}", rpc_addr.to_string());

        let tokens = 50;
        config.command = QtcCommand::AirDrop(tokens);
        assert_eq!(
            process_command(&config).unwrap(),
            format!("Your balance is: {:?}", tokens)
        );

        config.command = QtcCommand::Balance;
        assert_eq!(
            process_command(&config).unwrap(),
            format!("Your balance is: {:?}", tokens)
        );

        config.command = QtcCommand::Address;
        assert_eq!(
            process_command(&config).unwrap(),
            format!("{}", config.id.pubkey())
        );

        config.command = QtcCommand::Pay(10, bob_pubkey, None, None, None, None);
        let sig_response = process_command(&config);
        assert!(sig_response.is_ok());

        let signatures = bs58::decode(sig_response.unwrap())
            .into_vec()
            .expect("base58-encoded signature");
        let signature = Signature::new(&signatures);
        config.command = QtcCommand::Confirm(signature);
        assert_eq!(process_command(&config).unwrap(), "Confirmed");

        config.command = QtcCommand::Balance;
        assert_eq!(
            process_command(&config).unwrap(),
            format!("Your balance is: {:?}", tokens - 10)
        );

        server.close().unwrap();
        remove_dir_all(ledger_path).unwrap();
    }
    #[test]
    fn test_qtc_request_airdrop() {
        let leader_keypair = Keypair::new();
        let leader = Node::new_localhost_with_pubkey(leader_keypair.pubkey());

        let alice = Mint::new(10_000_000);
        let transaction_processor = TransactionProcessor::new(&alice);
        let bob_pubkey = Keypair::new().pubkey();
        let leader_data = leader.info.clone();
        let ledger_path = tmp_ledger("qtc_request_airdrop", &alice);

        let rpc_port = 11111; 
        let server = Fullnode::new_with_transaction_processor(
            leader_keypair,
            transaction_processor,
            0,
            &[],
            leader,
            None,
            &ledger_path,
            false,
            None,
            Some(rpc_port),
        );
        sleep(Duration::from_millis(900));

        let (sender, receiver) = channel();
        run_local_faucet(alice.keypair(), leader_data.contact_info.ncp, sender);
        let faucet_addr = receiver.recv().unwrap();

        let mut addr = leader_data.contact_info.ncp;
        addr.set_port(rpc_port);
        let rpc_addr = format!("http://{}", addr.to_string());

        let signature = request_airdrop(&faucet_addr, &bob_pubkey, 50);
        assert!(signature.is_ok());
        let params = json!(format!("{}", signature.unwrap()));
        let confirmation = QtcRpcRequest::ConfirmTransaction
            .make_rpc_request(&rpc_addr, 1, Some(params))
            .unwrap()
            .as_bool()
            .unwrap();
        assert!(confirmation);

        server.close().unwrap();
        remove_dir_all(ledger_path).unwrap();
    }
    #[test]
    fn test_qtc_gen_keypair_file() {
        let outfile = "test_gen_keypair_file.json";
        let serialized_keypair = gen_keypair_file(outfile.to_string()).unwrap();
        let keypair_vec: Vec<u8> = serde_json::from_str(&serialized_keypair).unwrap();
        assert!(Path::new(outfile).exists());
        assert_eq!(keypair_vec, read_pkcs8(&outfile).unwrap());
        assert!(read_keypair(&outfile).is_ok());
        assert_eq!(
            read_keypair(&outfile).unwrap().pubkey().as_ref().len(),
            mem::size_of::<Pubkey>()
        );
        fs::remove_file(outfile).unwrap();
        assert!(!Path::new(outfile).exists());
    }
    #[test]
    #[ignore]
    fn test_qtc_timestamp_tx() {
        let leader_keypair = Keypair::new();
        let leader = Node::new_localhost_with_pubkey(leader_keypair.pubkey());

        let alice = Mint::new(10_000_000);
        let transaction_processor = TransactionProcessor::new(&alice);
        let bob_pubkey = Keypair::new().pubkey();
        let leader_data = leader.info.clone();
        let leader_data1 = leader.info.clone();
        let leader_data2 = leader.info.clone();
        let ledger_path = tmp_ledger("qtc_timestamp_tx", &alice);

        let mut config_payer = QtcConfig::default();
        let mut config_witness = QtcConfig::default();
        let rpc_port = 13579; 
        let server = Fullnode::new_with_transaction_processor(
            leader_keypair,
            transaction_processor,
            0,
            &[],
            leader,
            None,
            &ledger_path,
            false,
            None,
            Some(rpc_port),
        );
        sleep(Duration::from_millis(900));

        let (sender, receiver) = channel();
        run_local_faucet(alice.keypair(), leader_data.contact_info.ncp, sender);
        config_payer.faucet_addr = receiver.recv().unwrap();
        config_witness.faucet_addr = config_payer.faucet_addr.clone();
        config_payer.leader = leader_data1;
        config_witness.leader = leader_data2;

        let mut rpc_addr = leader_data.contact_info.ncp;
        rpc_addr.set_port(rpc_port);
        config_payer.rpc_addr = format!("http://{}", rpc_addr.to_string());
        config_witness.rpc_addr = config_payer.rpc_addr.clone();

        assert_ne!(config_payer.id.pubkey(), config_witness.id.pubkey());

        let _signature = request_airdrop(&config_payer.faucet_addr, &config_payer.id.pubkey(), 50);

        let date_string = "\"2018-09-19T17:30:59Z\"";
        let dt: DateTime<Utc> = serde_json::from_str(&date_string).unwrap();
        config_payer.command = QtcCommand::Pay(
            10,
            bob_pubkey,
            Some(dt),
            Some(config_witness.id.pubkey()),
            None,
            None,
        );
        let sig_response = process_command(&config_payer);
        assert!(sig_response.is_ok());

        let object: Value = serde_json::from_str(&sig_response.unwrap()).unwrap();
        let process_id_str = object.get("processId").unwrap().as_str().unwrap();
        let process_id_vec = bs58::decode(process_id_str)
            .into_vec()
            .expect("base58-encoded public key");
        let process_id = Pubkey::new(&process_id_vec);

        let params = json!(format!("{}", config_payer.id.pubkey()));
        let config_payer_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(config_payer_balance, 39);
        let params = json!(format!("{}", process_id));
        let contract_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(contract_balance, 11);
        let params = json!(format!("{}", bob_pubkey));
        let recipient_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(recipient_balance, 0);

        config_witness.command = QtcCommand::TimeElapsed(bob_pubkey, process_id, dt);
        let sig_response = process_command(&config_witness);
        assert!(sig_response.is_ok());

        let params = json!(format!("{}", config_payer.id.pubkey()));
        let config_payer_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(config_payer_balance, 39);
        let params = json!(format!("{}", process_id));
        let contract_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(contract_balance, 1);
        let params = json!(format!("{}", bob_pubkey));
        let recipient_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(recipient_balance, 10);

        server.close().unwrap();
        remove_dir_all(ledger_path).unwrap();
    }
    #[test]
    #[ignore]
    fn test_qtc_witness_tx() {
        let leader_keypair = Keypair::new();
        let leader = Node::new_localhost_with_pubkey(leader_keypair.pubkey());

        let alice = Mint::new(10_000_000);
        let transaction_processor = TransactionProcessor::new(&alice);
        let bob_pubkey = Keypair::new().pubkey();
        let leader_data = leader.info.clone();
        let leader_data1 = leader.info.clone();
        let leader_data2 = leader.info.clone();
        let ledger_path = tmp_ledger("qtc_witness_tx", &alice);

        let mut config_payer = QtcConfig::default();
        let mut config_witness = QtcConfig::default();
        let rpc_port = 11223; // Needs to be distinct known number to not conflict with other tests

        let server = Fullnode::new_with_transaction_processor(
            leader_keypair,
            transaction_processor,
            0,
            &[],
            leader,
            None,
            &ledger_path,
            false,
            None,
            Some(rpc_port),
        );
        sleep(Duration::from_millis(900));

        let (sender, receiver) = channel();
        run_local_faucet(alice.keypair(), leader_data.contact_info.ncp, sender);
        config_payer.faucet_addr = receiver.recv().unwrap();
        config_witness.faucet_addr = config_payer.faucet_addr.clone();
        config_payer.leader = leader_data1;
        config_witness.leader = leader_data2;

        let mut rpc_addr = leader_data.contact_info.ncp;
        rpc_addr.set_port(rpc_port);
        config_payer.rpc_addr = format!("http://{}", rpc_addr.to_string());
        config_witness.rpc_addr = config_payer.rpc_addr.clone();

        assert_ne!(config_payer.id.pubkey(), config_witness.id.pubkey());

        let _signature = request_airdrop(&config_payer.faucet_addr, &config_payer.id.pubkey(), 50);

        config_payer.command = QtcCommand::Pay(
            10,
            bob_pubkey,
            None,
            None,
            Some(vec![config_witness.id.pubkey()]),
            None,
        );
        let sig_response = process_command(&config_payer);
        assert!(sig_response.is_ok());

        let object: Value = serde_json::from_str(&sig_response.unwrap()).unwrap();
        let process_id_str = object.get("processId").unwrap().as_str().unwrap();
        let process_id_vec = bs58::decode(process_id_str)
            .into_vec()
            .expect("base58-encoded public key");
        let process_id = Pubkey::new(&process_id_vec);

        let params = json!(format!("{}", config_payer.id.pubkey()));
        let config_payer_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(config_payer_balance, 39);
        let params = json!(format!("{}", process_id));
        let contract_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(contract_balance, 11);
        let params = json!(format!("{}", bob_pubkey));
        let recipient_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(recipient_balance, 0);

        config_witness.command = QtcCommand::Witness(bob_pubkey, process_id);
        let sig_response = process_command(&config_witness);
        assert!(sig_response.is_ok());

        let params = json!(format!("{}", config_payer.id.pubkey()));
        let config_payer_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(config_payer_balance, 39);
        let params = json!(format!("{}", process_id));
        let contract_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(contract_balance, 1);
        let params = json!(format!("{}", bob_pubkey));
        let recipient_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(recipient_balance, 10);

        server.close().unwrap();
        remove_dir_all(ledger_path).unwrap();
    }
    #[test]
    #[ignore]
    fn test_qtc_cancel_tx() {
        let leader_keypair = Keypair::new();
        let leader = Node::new_localhost_with_pubkey(leader_keypair.pubkey());

        let alice = Mint::new(10_000_000);
        let transaction_processor = TransactionProcessor::new(&alice);
        let bob_pubkey = Keypair::new().pubkey();
        let leader_data = leader.info.clone();
        let leader_data1 = leader.info.clone();
        let leader_data2 = leader.info.clone();
        let ledger_path = tmp_ledger("qtc_cancel_tx", &alice);

        let mut config_payer = QtcConfig::default();
        let mut config_witness = QtcConfig::default();
        let rpc_port = 13456; // Needs to be distinct known number to not conflict with other tests

        let server = Fullnode::new_with_transaction_processor(
            leader_keypair,
            transaction_processor,
            0,
            &[],
            leader,
            None,
            &ledger_path,
            false,
            None,
            Some(rpc_port),
        );
        sleep(Duration::from_millis(900));

        let (sender, receiver) = channel();
        run_local_faucet(alice.keypair(), leader_data.contact_info.ncp, sender);
        config_payer.faucet_addr = receiver.recv().unwrap();
        config_witness.faucet_addr = config_payer.faucet_addr.clone();
        config_payer.leader = leader_data1;
        config_witness.leader = leader_data2;

        let mut rpc_addr = leader_data.contact_info.ncp;
        rpc_addr.set_port(rpc_port);
        config_payer.rpc_addr = format!("http://{}", rpc_addr.to_string());
        config_witness.rpc_addr = config_payer.rpc_addr.clone();

        assert_ne!(config_payer.id.pubkey(), config_witness.id.pubkey());

        let _signature = request_airdrop(&config_payer.faucet_addr, &config_payer.id.pubkey(), 50);

        // Make transaction (from config_payer to bob_pubkey) requiring witness signature from config_witness
        config_payer.command = QtcCommand::Pay(
            10,
            bob_pubkey,
            None,
            None,
            Some(vec![config_witness.id.pubkey()]),
            Some(config_payer.id.pubkey()),
        );
        let sig_response = process_command(&config_payer);
        assert!(sig_response.is_ok());

        let object: Value = serde_json::from_str(&sig_response.unwrap()).unwrap();
        let process_id_str = object.get("processId").unwrap().as_str().unwrap();
        let process_id_vec = bs58::decode(process_id_str)
            .into_vec()
            .expect("base58-encoded public key");
        let process_id = Pubkey::new(&process_id_vec);

        let params = json!(format!("{}", config_payer.id.pubkey()));
        let config_payer_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(config_payer_balance, 39);
        let params = json!(format!("{}", process_id));
        let contract_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(contract_balance, 11);
        let params = json!(format!("{}", bob_pubkey));
        let recipient_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(recipient_balance, 0);

        // Sign transaction by config_witness
        config_payer.command = QtcCommand::Cancel(process_id);
        let sig_response = process_command(&config_payer);
        assert!(sig_response.is_ok());

        let params = json!(format!("{}", config_payer.id.pubkey()));
        let config_payer_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(config_payer_balance, 49);
        let params = json!(format!("{}", process_id));
        let contract_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(contract_balance, 1);
        let params = json!(format!("{}", bob_pubkey));
        let recipient_balance = QtcRpcRequest::GetBalance
            .make_rpc_request(&config_payer.rpc_addr, 1, Some(params))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(recipient_balance, 0);

        server.close().unwrap();
        remove_dir_all(ledger_path).unwrap();
    }
}
