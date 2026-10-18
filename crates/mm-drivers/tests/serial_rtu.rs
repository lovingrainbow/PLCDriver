//! 序列埠 RTU 驅動測試。
//!
//! 以 pty 配對模擬 PLC（不是實機）：從端執行緒持有 pty master，驅動開啟 slave 路徑。
//! pty 沒有真實鮑率／RS-485 方向控制，所以 19200-8-E-1 與 RTS 時序皆「未驗證」。
//! 沒有任何一項測試針對實體 PLC 驗證過。
#![cfg(unix)]

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use plc_common::ScalarValue;
use mm_drivers::{
    modbus_crc16, DriverConfig, DriverError, DriverKind, PlcDriver, SerialPortConfig, UniversalPlcDriver,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn with_crc(mut body: Vec<u8>) -> Vec<u8> {
    let crc = modbus_crc16(&body);
    body.extend_from_slice(&crc);
    body
}

/// 一次回覆由多個片段組成（片段間隔 10ms）；空 Vec 代表不回應。
type Reply = Vec<Vec<u8>>;

/// 連線交握用的合法回應（D100 = 0x007B）。
fn handshake_ok() -> Reply {
    vec![with_crc(vec![0x01, 0x03, 0x02, 0x00, 0x7B])]
}

/// 啟動模擬 PLC，回傳 slave 路徑。第 N 個請求依序取第 N 個回覆。
fn spawn_plc(replies: Vec<Reply>) -> String {
    let (path_tx, path_rx) = mpsc::channel();
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("rt");
        rt.block_on(async move {
            let (mut master, slave) = tokio_serial::SerialStream::pair().expect("pty pair");
            use tokio_serial::SerialPort;
            let name = slave.name().expect("slave name");
            path_tx.send(name).expect("send path");
            let mut buf = [0u8; 256];
            for reply in replies {
                let read = tokio::time::timeout(Duration::from_secs(5), master.read(&mut buf)).await;
                match read {
                    Ok(Ok(n)) if n > 0 => {}
                    _ => break,
                }
                for chunk in reply {
                    let _ = master.write_all(&chunk).await;
                    let _ = master.flush().await;
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
            // 保持 pty 開著，讓驅動端有時間讀完
            tokio::time::sleep(Duration::from_millis(1500)).await;
            drop(slave);
        });
    });
    path_rx.recv_timeout(Duration::from_secs(5)).expect("pty path")
}

fn serial_cfg(path: &str, timeout_ms: u64) -> DriverConfig {
    DriverConfig {
        kind: DriverKind::InovanceEasySerial,
        loopback_emulation: false,
        timeout_ms,
        serial: Some(SerialPortConfig {
            port_name: path.into(),
            // 19200-8-E-1 為預設值，未經實機驗證；RS-485 RTS 時序同為未驗證
            rs485_rts_enable: false,
            inter_frame_silence_ms: 50,
            ..SerialPortConfig::default()
        }),
        ..DriverConfig::default()
    }
}

fn connected(replies: Vec<Reply>, timeout_ms: u64) -> UniversalPlcDriver {
    let path = spawn_plc(replies);
    let cfg = serial_cfg(&path, timeout_ms);
    let mut drv = UniversalPlcDriver::new(cfg.clone());
    drv.connect(cfg).expect("connect");
    drv
}

#[test]
fn serial_read_returns_register_value() {
    let mut drv = connected(vec![handshake_ok(), handshake_ok()], 1000);
    let r = drv.read_single("D100", "int16").expect("read");
    assert_eq!(r.value, ScalarValue::Number(123.0));
}

#[test]
fn serial_exception_frame_is_error() {
    let exc = vec![with_crc(vec![0x01, 0x83, 0x02])];
    let mut drv = connected(vec![handshake_ok(), exc], 1000);
    match drv.read_single("D100", "int16") {
        Err(DriverError::ProtocolError(msg)) => assert!(msg.contains("Exception"), "應為 Modbus 例外回應: {msg}"),
        other => panic!("應為 ProtocolError: {other:?}"),
    }
}

#[test]
fn serial_bad_crc_is_error() {
    let mut bad = with_crc(vec![0x01, 0x03, 0x02, 0x00, 0x7B]);
    let last = bad.len() - 1;
    bad[last] ^= 0x01;
    let mut drv = connected(vec![handshake_ok(), vec![bad]], 1000);
    match drv.read_single("D100", "int16") {
        Err(DriverError::ProtocolError(msg)) => assert!(msg.to_uppercase().contains("CRC"), "訊息應提及 CRC: {msg}"),
        other => panic!("應為 ProtocolError: {other:?}"),
    }
}

#[test]
fn reconnect_releases_previous_serial_port() {
    let path = spawn_plc(vec![handshake_ok(), handshake_ok()]);
    let cfg = serial_cfg(&path, 1000);
    let mut drv = UniversalPlcDriver::new(cfg.clone());
    drv.connect(cfg.clone()).expect("first connect");
    drv.connect(cfg).expect("reconnect");
    assert!(drv.is_connected());
}

#[test]
fn serial_silent_port_times_out() {
    let mut drv = connected(vec![handshake_ok(), vec![]], 100);
    let t0 = Instant::now();
    let r = drv.read_single("D100", "int16");
    assert!(r.is_err());
    assert!(t0.elapsed() < Duration::from_secs(1), "應於 1 秒內逾時");
}

#[test]
fn serial_fragmented_response_is_joined() {
    let full = with_crc(vec![0x01, 0x03, 0x02, 0x00, 0x7B]);
    let (a, b) = full.split_at(3);
    let frag = vec![a.to_vec(), b.to_vec()];
    let mut drv = connected(vec![handshake_ok(), frag], 1000);
    let r = drv.read_single("D100", "int16").expect("read");
    assert_eq!(r.value, ScalarValue::Number(123.0));
}

#[test]
fn serial_without_config_is_error() {
    let cfg = DriverConfig {
        kind: DriverKind::InovanceEasySerial,
        loopback_emulation: false,
        serial: None,
        ..DriverConfig::default()
    };
    let mut drv = UniversalPlcDriver::new(cfg.clone());
    assert!(matches!(drv.connect(cfg), Err(DriverError::InvalidAddress(_))));
}

#[test]
fn serial_missing_port_is_error() {
    let cfg = serial_cfg("/dev/does-not-exist", 100);
    let mut drv = UniversalPlcDriver::new(cfg.clone());
    assert!(matches!(drv.connect(cfg), Err(DriverError::IoError(_))));
}

#[test]
fn loopback_emulation_needs_no_port() {
    let cfg = DriverConfig {
        kind: DriverKind::InovanceEasySerial,
        loopback_emulation: true,
        serial: None,
        ..DriverConfig::default()
    };
    let mut drv = UniversalPlcDriver::new(cfg.clone());
    drv.connect(cfg).expect("connect");
    assert!(drv.read_single("D100", "int16").is_ok());
}
