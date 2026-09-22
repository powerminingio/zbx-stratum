use clap::Parser;
use serde::Serialize;
use std::{
    convert::TryInto,
    time::Duration,
};
use stratum_apps::stratum_core::{
    binary_sv2::{Deserialize, Str0255, U256},
    codec_sv2::{HandshakeRole, StandardEitherFrame, StandardSv2Frame},
    common_messages_sv2::{
        Protocol, SetupConnection, SetupConnectionError, SetupConnectionSuccess,
        MESSAGE_TYPE_SETUP_CONNECTION_ERROR, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
    },
    mining_sv2::{
        OpenMiningChannelError, OpenStandardMiningChannel, OpenStandardMiningChannelSuccess,
        MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH, MESSAGE_TYPE_NEW_MINING_JOB,
        MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR, MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
    },
    noise_sv2::Initiator,
    parsers_sv2::{AnyMessage, Mining},
};
use stratum_apps::network_helpers::noise_stream::NoiseTcpStream;
use tokio::{net::TcpStream, time::{timeout_at, Instant}};

type Message = AnyMessage<'static>;
type Sv2Frame = StandardSv2Frame<Message>;
type EitherFrame = StandardEitherFrame<Message>;

#[derive(Parser, Debug)]
#[command(version, about = "Stratum V2 health probe for Zabbix")]
struct Args {
    host: String,
    port: u16,

    #[arg(long, default_value_t = 3.0)]
    timeout: f64,

    #[arg(long, default_value = "zabbix")]
    user: String,

    #[arg(long, default_value_t = 1_000_000_000_000.0)]
    hashrate: f32,

    /// Pool authority public key in Stratum V2 Base58Check format.
    /// This is the key published by the pool, typically ~51 characters.
    /// If omitted, Noise is encrypted but the pool authority is not authenticated.
    #[arg(long)]
    authority_key: Option<String>,

    /// How long to wait for NewMiningJob / SetNewPrevHash after channel open.
    #[arg(long, default_value_t = 0.8)]
    job_wait: f64,
}

#[derive(Serialize, Debug)]
struct ProbeResult {
    alive: u8,
    tcp_ok: u8,
    noise_ok: u8,
    setup_ok: u8,
    channel_ok: u8,
    job_seen: u8,
    prevhash_seen: u8,
    latency_ms: u64,
    used_version: u16,
    setup_flags: u32,
    channel_id: u32,
    authority_verified: u8,
    error_stage: String,
    error_code: String,
}

impl Default for ProbeResult {
    fn default() -> Self {
        Self {
            alive: 0,
            tcp_ok: 0,
            noise_ok: 0,
            setup_ok: 0,
            channel_ok: 0,
            job_seen: 0,
            prevhash_seen: 0,
            latency_ms: 0,
            used_version: 0,
            setup_flags: 0,
            channel_id: 0,
            authority_verified: 0,
            error_stage: String::new(),
            error_code: String::new(),
        }
    }
}

fn fail(mut r: ProbeResult, stage: &str, code: impl ToString, start: Instant) -> ProbeResult {
    r.error_stage = stage.to_string();
    r.error_code = code.to_string();
    r.latency_ms = start.elapsed().as_millis() as u64;
    r
}

fn str255(s: &str) -> Result<Str0255<'static>, String> {
    s.to_string()
        .try_into()
        .map_err(|_| format!("string-too-long: {s}"))
}

fn parse_authority_key(s: &str) -> Result<[u8; 32], String> {
    let decoded = bs58::decode(s)
        .with_check(None)
        .into_vec()
        .map_err(|e| format!("invalid-authority-key-base58check: {e}"))?;

    // SV2 authority key encoding:
    // [1, 0] || 32-byte x-only secp256k1 pubkey
    if decoded.len() != 34 {
        return Err(format!(
            "invalid-authority-key-length: expected 34 decoded bytes, got {}",
            decoded.len()
        ));
    }

    if decoded[0] != 1 || decoded[1] != 0 {
        return Err(format!(
            "unsupported-authority-key-version: {:02x}{:02x}",
            decoded[0], decoded[1]
        ));
    }

    decoded[2..34]
        .try_into()
        .map_err(|_| "failed-to-extract-authority-public-key".to_string())
}

fn max_target() -> Result<U256<'static>, String> {
    let bytes = vec![0xffu8; 32];
    bytes
        .try_into()
        .map_err(|_| "failed-to-build-max-target".to_string())
}

fn message_to_frame(message: Message) -> Result<EitherFrame, String> {
    let frame: Sv2Frame = message
        .try_into()
        .map_err(|e| format!("frame-encode: {e:?}"))?;
    Ok(frame.into())
}

fn into_sv2_frame(frame: EitherFrame) -> Result<Sv2Frame, String> {
    frame
        .try_into()
        .map_err(|e| format!("expected-sv2-frame: {e:?}"))
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let result = run(args).await;
    println!(
        "{}",
        serde_json::to_string(&result).unwrap_or_else(|_| "{\"alive\":0}".to_string())
    );
}

async fn run(args: Args) -> ProbeResult {
    let start = Instant::now();
    let mut r = ProbeResult::default();
    let total = Duration::from_secs_f64(args.timeout.max(0.1));
    let deadline = start + total;

    let tcp = match timeout_at(deadline, TcpStream::connect((args.host.as_str(), args.port))).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return fail(r, "tcp", e, start),
        Err(_) => return fail(r, "tcp", "timeout", start),
    };
    r.tcp_ok = 1;

    let authority_requested = args
        .authority_key
        .as_deref()
        .is_some_and(|key| !key.is_empty());

    let initiator = match args.authority_key.as_deref() {
        Some(k) if !k.is_empty() => match parse_authority_key(k).and_then(|raw| {
            Initiator::from_raw_k(raw).map_err(|e| format!("invalid-authority-key: {e:?}"))
        }) {
            Ok(i) => i,
            Err(e) => return fail(r, "noise", e, start),
        },
        _ => match Initiator::without_pk() {
            Ok(i) => i,
            Err(e) => return fail(r, "noise", format!("initiator: {e:?}"), start),
        },
    };

    let role = HandshakeRole::Initiator(initiator);
    let remain = deadline.saturating_duration_since(Instant::now());
    let noise = match timeout_at(deadline, NoiseTcpStream::<Message>::new(tcp, role, remain)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return fail(r, "noise", e, start),
        Err(_) => return fail(r, "noise", "timeout", start),
    };
    r.noise_ok = 1;
    if authority_requested {
        // A successful authenticated Noise handshake means the supplied authority key
        // accepted the responder certificate/signature.
        r.authority_verified = 1;
    }
    let (mut reader, mut writer) = noise.into_split();

    let mut setup = SetupConnection {
        protocol: Protocol::MiningProtocol,
        min_version: 2,
        max_version: 2,
        flags: 0,
        endpoint_host: match str255(&args.host) {
            Ok(v) => v,
            Err(e) => return fail(r, "setup", e, start),
        },
        endpoint_port: args.port,
        vendor: str255("zbx-stratum").unwrap(),
        hardware_version: str255("probe").unwrap(),
        firmware: str255(env!("CARGO_PKG_VERSION")).unwrap(),
        device_id: str255("").unwrap(),
    };
    setup.set_requires_standard_job();

    let frame = match message_to_frame(setup.into()) {
        Ok(v) => v,
        Err(e) => return fail(r, "setup", e, start),
    };
    match timeout_at(deadline, writer.write_frame(frame)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return fail(r, "setup", e, start),
        Err(_) => return fail(r, "setup", "timeout-writing-setup", start),
    }

    let mut setup_frame = match timeout_at(deadline, reader.read_frame()).await {
        Ok(Ok(v)) => match into_sv2_frame(v) {
            Ok(frame) => frame,
            Err(e) => return fail(r, "setup", e, start),
        },
        Ok(Err(e)) => return fail(r, "setup", e, start),
        Err(_) => return fail(r, "setup", "timeout", start),
    };
    let setup_msg_type = match setup_frame.get_header() {
        Some(header) => header.msg_type(),
        None => return fail(r, "setup", "missing-frame-header", start),
    };
    match setup_msg_type {
        MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS => {
            match SetupConnectionSuccess::from_bytes(setup_frame.payload()) {
                Ok(v) => {
                    r.setup_ok = 1;
                    r.used_version = v.used_version;
                    r.setup_flags = v.flags;
                }
                Err(e) => return fail(r, "setup", format!("decode: {e:?}"), start),
            }
        }
        MESSAGE_TYPE_SETUP_CONNECTION_ERROR => {
            match SetupConnectionError::from_bytes(setup_frame.payload()) {
                Ok(v) => return fail(r, "setup", v.error_code.as_utf8_or_hex(), start),
                Err(e) => {
                    return fail(r, "setup", format!("setup-error-decode: {e:?}"), start)
                }
            }
        }
        other => {
            return fail(
                r,
                "setup",
                format!("unexpected-message-type-0x{other:02x}"),
                start,
            )
        }
    }

    let open = OpenStandardMiningChannel {
        request_id: 1,
        user_identity: match str255(&args.user) {
            Ok(v) => v,
            Err(e) => return fail(r, "channel", e, start),
        },
        nominal_hash_rate: args.hashrate,
        max_target: match max_target() {
            Ok(v) => v,
            Err(e) => return fail(r, "channel", e, start),
        },
    };
    let open_message = Message::Mining(Mining::OpenStandardMiningChannel(open));
    let frame = match message_to_frame(open_message) {
        Ok(v) => v,
        Err(e) => return fail(r, "channel", e, start),
    };
    match timeout_at(deadline, writer.write_frame(frame)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return fail(r, "channel", e, start),
        Err(_) => return fail(r, "channel", "timeout-writing-open-channel", start),
    }

    while Instant::now() < deadline {
        let mut f = match timeout_at(deadline, reader.read_frame()).await {
            Ok(Ok(v)) => match into_sv2_frame(v) {
                Ok(frame) => frame,
                Err(e) => return fail(r, "channel", e, start),
            },
            Ok(Err(e)) => return fail(r, "channel", e, start),
            Err(_) => return fail(r, "channel", "timeout", start),
        };
        let msg_type = match f.get_header() {
            Some(header) => header.msg_type(),
            None => return fail(r, "channel", "missing-frame-header", start),
        };
        match msg_type {
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS => {
                match OpenStandardMiningChannelSuccess::from_bytes(f.payload()) {
                    Ok(v) => {
                        r.channel_ok = 1;
                        r.channel_id = v.channel_id;
                        break;
                    }
                    Err(e) => return fail(r, "channel", format!("decode: {e:?}"), start),
                }
            }
            MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR => {
                match OpenMiningChannelError::from_bytes(f.payload()) {
                    Ok(v) => return fail(r, "channel", v.error_code.as_utf8_or_hex(), start),
                    Err(e) => {
                        return fail(r, "channel", format!("open-error-decode: {e:?}"), start)
                    }
                }
            }
            MESSAGE_TYPE_NEW_MINING_JOB => r.job_seen = 1,
            MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH => r.prevhash_seen = 1,
            _ => {}
        }
    }

    if r.channel_ok == 0 {
        return fail(r, "channel", "no-open-channel-success", start);
    }

    let listen = Duration::from_secs_f64(args.job_wait.max(0.0));
    let job_deadline = Instant::now() + listen;
    let read_deadline = std::cmp::min(job_deadline, deadline);
    while Instant::now() < read_deadline {
        let mut f = match timeout_at(read_deadline, reader.read_frame()).await {
            Ok(Ok(v)) => match into_sv2_frame(v) {
                Ok(frame) => frame,
                Err(_) => break,
            },
            Ok(Err(_)) | Err(_) => break,
        };
        let msg_type = match f.get_header() {
            Some(header) => header.msg_type(),
            None => break,
        };
        match msg_type {
            MESSAGE_TYPE_NEW_MINING_JOB => r.job_seen = 1,
            MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH => r.prevhash_seen = 1,
            _ => {}
        }
    }

    r.alive = 1;
    r.latency_ms = start.elapsed().as_millis() as u64;
    r
}
