//! # `driver-workbench` — Industrial PLC Driver CLI & Web Server
//!
//! Provides CLI benchmarking, direct real hardware I/O, and interactive REST Web Server.

use std::net::SocketAddr;
use std::sync::Arc;
use plc_common::ScalarValue;
use mm_drivers::{
    ByteOrder, DriverConfig, DriverKind, PlcDriver, RandomReadItem,
    SerialParity, SerialPortConfig, StressMode, StressTestConfig,
    TransportKind, UniversalPlcDriver, bytes_to_hex, list_driver_catalog,
    checksum::{crc16_modbus, lrc_byte, melsec_sum_check_ascii, omron_fcs_ascii, verify_crc16_modbus},
    inovance::{build_inovance_serial_read_frame, translate_inovance_address, InovanceSeries},
    melsec_fx::{build_fx_links_frame, build_fx_serial_read_frame, build_fx_serial_write_frame},
    omron::{build_fins_core_command, build_fins_multiple_read_command, build_fins_tcp_handshake_frame, pack_omron_hostlink_serial},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let is_help = args.iter().any(|a| a == "--help" || a == "-h");
    let serve_mode = args.iter().any(|a| a == "--serve" || a == "-s");
    let is_real = args.iter().any(|a| a == "--real" || a == "-r");

    let get_opt = |flag: &str| -> Option<String> {
        args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
    };

    let target_ip = get_opt("--ip").unwrap_or_else(|| "127.0.0.1".into());
    let target_port: Option<u16> = get_opt("--port").and_then(|p| p.parse().ok());
    let target_kind_str = get_opt("--kind");
    let read_addr = get_opt("--read");
    let write_pair = get_opt("--write");
    let data_type = get_opt("--type").unwrap_or_else(|| "int16".into());
    let read_count: u16 = get_opt("--count").and_then(|c| c.parse().ok()).unwrap_or(1);
    let web_port: u16 = get_opt("--web-port")
        .or_else(|| if !args.iter().any(|a| a == "--real") { get_opt("--port") } else { None })
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    if is_help {
        println!("================================================================================");
        println!("  ArgonGUI Industrial PLC Driver Workbench & Benchmark Runner");
        println!("================================================================================");
        println!("用法說明 (USAGE):");
        println!("  1. 預設本機回環模擬與 56 種驅動綜合壓測:");
        println!("       cargo run -p driver-workbench");
        println!();
        println!("  2. 啟動視覺化 Web 工作台與 REST API 伺服器 (支援真實/虛擬連線):");
        println!("       cargo run -p driver-workbench -- --serve [--web-port 8080]");
        println!();
        println!("  3. 連接真實實體設備 (Real Hardware) 執行單點讀取:");
        println!("       cargo run -p driver-workbench -- --real --ip 192.168.1.10 --port 502 --kind modbus_tcp --read D100 [--type int16]");
        println!();
        println!("  4. 連接真實實體設備執行單點寫入與回讀驗證:");
        println!("       cargo run -p driver-workbench -- --real --ip 192.168.1.10 --port 502 --kind modbus_tcp --write D100=3888 [--type int16]");
        println!();
        println!("  5. 連接真實實體設備執行連續區塊讀取 (Batch Read):");
        println!("       cargo run -p driver-workbench -- --real --ip 192.168.1.10 --port 502 --kind modbus_tcp --read D100 --count 16");
        println!("================================================================================");
        return Ok(());
    }

    // CLI Direct I/O Mode (Targeting either Real PLC or loopback)
    if read_addr.is_some() || write_pair.is_some() {
        let catalog = list_driver_catalog();
        let target_item = target_kind_str.as_ref().and_then(|k| {
            catalog.iter().find(|c| {
                format!("{:?}", c.kind).to_lowercase() == k.to_lowercase()
                    || c.csharp_class.to_lowercase().contains(&k.to_lowercase())
            }).cloned()
        }).unwrap_or_else(|| {
            catalog.iter().find(|c| c.kind == DriverKind::ModbusTcp).cloned().unwrap()
        });

        let port = target_port.unwrap_or(target_item.default_port);
        let mut cfg = DriverConfig::default();
        cfg.kind = target_item.kind;
        cfg.host = target_ip.clone();
        cfg.port = port;
        cfg.byte_order = target_item.default_byte_order;
        cfg.loopback_emulation = !is_real;

        println!("================================================================================");
        println!("  實體連線模式: {} (Kind: {:?}, Target: {}:{})",
            if is_real { "真實硬體設備 (Real Hardware via TCP)" } else { "虛擬站點 (Loopback Emulation)" },
            cfg.kind, cfg.host, cfg.port
        );
        println!("================================================================================");

        let mut driver = UniversalPlcDriver::new(cfg.clone());
        match driver.connect(cfg) {
            Ok(trace) => {
                println!("  ✓ 連線成功！耗時: {} µs, 交握 TX: {}, RX: {}", trace.elapsed_us, trace.tx_hex, trace.rx_hex);
            }
            Err(e) => {
                eprintln!("  ❌ 連線失敗: {}", e);
                return Ok(());
            }
        }

        if let Some(ref addr) = read_addr {
            if read_count > 1 {
                match driver.read_continuous(addr, read_count, &data_type) {
                    Ok(res) => {
                        println!("  ✓ 連續讀取 {} (長度: {}, 型別: {}):", addr, read_count, data_type);
                        println!("    數值: {:?}", res.values);
                        println!("    原始 Hex: {}", res.raw_bytes_hex);
                        println!("    耗時: {} µs (TX: {})", res.trace.elapsed_us, res.trace.tx_hex);
                    }
                    Err(e) => eprintln!("  ❌ 連續讀取失敗: {}", e),
                }
            } else {
                match driver.read_single(addr, &data_type) {
                    Ok(res) => {
                        println!("  ✓ 單點讀取 {} (型別: {}):", addr, data_type);
                        println!("    數值: {:?}", res.value);
                        println!("    品質: {:?}", res.quality);
                        println!("    耗時: {} µs (TX: {}, RX: {})", res.trace.elapsed_us, res.trace.tx_hex, res.trace.rx_hex);
                    }
                    Err(e) => eprintln!("  ❌ 單點讀取失敗: {}", e),
                }
            }
        }

        if let Some(ref pair) = write_pair {
            if let Some((addr, val_str)) = pair.split_once('=') {
                let val = if data_type == "bool" {
                    ScalarValue::Bool(val_str.parse().unwrap_or(true))
                } else if let Ok(num) = val_str.parse::<f64>() {
                    ScalarValue::Number(num)
                } else {
                    ScalarValue::Text(val_str.to_string())
                };

                match driver.write_single(addr, &data_type, val) {
                    Ok(ack) => {
                        println!("  ✓ 寫入成功 {}:", addr);
                        println!("    寫入值: {:?}, 回讀驗證值: {:?}, 一致: {}", ack.written_value, ack.readback_value, ack.readback_matched);
                        println!("    耗時: {} µs (TX: {}, RX: {})", ack.trace.elapsed_us, ack.trace.tx_hex, ack.trace.rx_hex);
                    }
                    Err(e) => eprintln!("  ❌ 寫入失敗: {}", e),
                }
            }
        }

        return Ok(());
    }

    println!("================================================================================");
    println!("  ArgonGUI Industrial PLC Driver Workbench & Benchmark Runner");
    println!("================================================================================");

    let catalog = list_driver_catalog();
    println!("Loaded {} supported Industrial Drivers (PLC, Modbus, Robot, CNC, Instrument):", catalog.len());
    for item in &catalog {
        println!("  - [{}] [{:?}] {} (Port: {}, Order: {:?})", item.category, item.kind, item.csharp_class, item.default_port, item.default_byte_order);
    }
    println!();

    // 1. Initialize Driver
    let cfg = DriverConfig::default();
    println!("Initializing driver with config: {:?} (Loopback Emulation: {})", cfg.kind, cfg.loopback_emulation);
    let mut driver = UniversalPlcDriver::new(cfg.clone());
    let trace = driver.connect(cfg)?;
    println!("Driver connected in {} µs (TX: {})", trace.elapsed_us, trace.tx_hex);
    println!();

    // 2. Single Read & Write Benchmark
    println!("--- 1. Single Read & Write with Read-back Verification ---");
    let read1 = driver.read_single("D120", "int16")?;
    println!("  Read D120: {:?} (Quality: {:?}, Latency: {} µs)", read1.value, read1.quality, read1.trace.elapsed_us);
    let ack1 = driver.write_single("D120", "int16", ScalarValue::Number(3888.0))?;
    println!("  Write D120 = 3888: Matched={}, Latency={} µs", ack1.readback_matched, ack1.trace.elapsed_us);
    let read2 = driver.read_single("D120", "int16")?;
    println!("  Verified D120: {:?}", read2.value);
    println!();

    // 3. Continuous Read & Write
    println!("--- 2. Continuous Block Read & Write (Batch 16 Words) ---");
    let cont_read = driver.read_continuous("D100", 8, "int16")?;
    println!("  Continuous Read D100~D107: values={:?}, bytes={}", cont_read.values, cont_read.raw_bytes_hex);
    let batch_vals: Vec<ScalarValue> = (0..8).map(|i| ScalarValue::Number(f64::from(1000 + i * 50))).collect();
    let cont_write = driver.write_continuous("D100", "int16", &batch_vals)?;
    println!("  Continuous Write D100~D107: {} µs", cont_write.trace.elapsed_us);
    let cont_read2 = driver.read_continuous("D100", 8, "int16")?;
    println!("  Verified Read D100~D107: values={:?}", cont_read2.values);
    println!();

    // 4. Random Scatter-Gather Read & Write
    println!("--- 3. Multi-point Random Read (Scatter-Gather) ---");
    let random_items = vec![
        RandomReadItem { address: "D120".into(), data_type: "int16".into() },
        RandomReadItem { address: "D150".into(), data_type: "float32".into() },
        RandomReadItem { address: "M104".into(), data_type: "bool".into() },
        RandomReadItem { address: "X000".into(), data_type: "bool".into() },
    ];
    let random_res = driver.read_random(&random_items)?;
    for it in &random_res.items {
        println!("  - Random Item {}: {:?} (Latency: {} µs)", it.address, it.value, it.trace.elapsed_us);
    }
    println!("  Total random read batch latency: {} µs", random_res.total_elapsed_us);
    println!();

    // 5. Latency Profiling (100 rounds)
    println!("--- 4. Latency Profiling (RTT Ping 100 rounds on D150) ---");
    let lat_report = driver.ping_latency("D150", 100)?;
    println!("  Rounds: {}", lat_report.rounds);
    println!("  Min / Avg / Max: {} µs / {} µs / {} µs", lat_report.min_us, lat_report.avg_us, lat_report.max_us);
    println!("  P50 / P95 / P99: {} µs / {} µs / {} µs", lat_report.p50_us, lat_report.p95_us, lat_report.p99_us);
    println!("  Jitter: {} µs, Success: {}/{}", lat_report.jitter_us, lat_report.success_count, lat_report.rounds);
    println!();

    // 6. Stress Test Benchmark (2,000 operations)
    println!("--- 5. High-concurrency Stress Test Benchmark ---");
    let stress_cfg = StressTestConfig {
        mode: StressMode::ContinuousRead,
        address: "D100".into(),
        length: 64,
        concurrency: 1,
        iterations: 2000,
    };
    let stress_report = driver.run_stress_test(&stress_cfg)?;
    println!("  Mode: {:?}", stress_report.mode);
    println!("  Total Operations: {} (Success: {}, Failed: {})", stress_report.total_ops, stress_report.success_ops, stress_report.failed_ops);
    println!("  Total Duration: {:.2} ms", stress_report.total_duration_ms);
    println!("  Throughput: {:.0} ops/sec, Bandwidth: {:.1} KB/sec", stress_report.throughput_ops_per_sec, stress_report.throughput_kb_per_sec);
    println!("  Latencies: Min={} µs, Avg={} µs, P95={} µs, P99={} µs, Max={} µs",
        stress_report.min_us, stress_report.avg_us, stress_report.p95_us, stress_report.p99_us, stress_report.max_us
    );
    println!();

    // 7. Serial and Depth Protocol Demonstration
    println!("--- 6. New Serial Transport & Protocol Engines Demonstration ---");
    let ser_cfg = SerialPortConfig {
        port_name: "COM3".into(),
        baud_rate: 19200,
        data_bits: 8,
        parity: SerialParity::Even,
        stop_bits: 1,
        rs485_rts_enable: true,
        inter_frame_silence_ms: 5,
    };
    println!("  [Serial Transport] Kind: {:?}, Port: {}, Baud: {}, Format: {}-{:?}-{}, RTS: {}",
        TransportKind::SerialRs485, ser_cfg.port_name, ser_cfg.baud_rate, ser_cfg.data_bits, ser_cfg.parity, ser_cfg.stop_bits, ser_cfg.rs485_rts_enable
    );

    // 7.2 Industrial Checksum Engines
    let test_rtu = [0x01, 0x03, 0x00, 0x64, 0x00, 0x02];
    let crc = crc16_modbus(&test_rtu);
    let crc_valid = verify_crc16_modbus(&[0x01, 0x03, 0x00, 0x64, 0x00, 0x02, crc[0], crc[1]]);
    let lrc = lrc_byte(&test_rtu);
    let fx_sum = melsec_sum_check_ascii(b"0100002");
    let omron_fcs = omron_fcs_ascii(b"@00RD00000001");
    println!("  [Checksums] Modbus CRC-16: {} (Verified: {}) | LRC: {:02X} | FX SumCheck: {} | Omron FCS: {}",
        bytes_to_hex(&crc), crc_valid, lrc, String::from_utf8_lossy(&fx_sum), String::from_utf8_lossy(&omron_fcs)
    );

    // 7.3 Mitsubishi FX Serial Read & Force Commands
    let fx_read = build_fx_serial_read_frame("D100", 2, false)?;
    let fx_force = build_fx_serial_write_frame("M100", true, true, &[], false)?;
    let fx_links = build_fx_links_frame(0, "D100", 1, None, true)?;
    println!("  [Mitsubishi FX] FX3U Serial Read Frame: {}", bytes_to_hex(&fx_read));
    println!("  [Mitsubishi FX] FX Force ON M100:       {}", bytes_to_hex(&fx_force));
    println!("  [Mitsubishi FX] FX Links 485BD WR:       {}", String::from_utf8_lossy(&fx_links).trim());

    // 7.4 Inovance Address Translation & Serial Frame Builder
    let (_, easy_x10) = translate_inovance_address(InovanceSeries::Easy, "X010")?;
    let (_, h3u_sd0) = translate_inovance_address(InovanceSeries::H3U, "SD0")?;
    let (_, am_mw100) = translate_inovance_address(InovanceSeries::AM, "MW100")?;
    let (modbus_reg, ino_frame) = build_inovance_serial_read_frame(InovanceSeries::H5U, 1, "D100", 2, false)?;
    println!("  [Inovance Mapping] Easy X010 -> 0x{:04X} | H3U SD0 -> 0x{:04X} | AM MW100 -> 0x{:04X}", easy_x10, h3u_sd0, am_mw100);
    println!("  [Inovance RTU] H5U Read D100 (Modbus 0x{:04X}) Frame: {}", modbus_reg, bytes_to_hex(&ino_frame));

    // 7.5 Omron FINS 20B Handshake, 01 04 Multi-area Read & HostLink Packet
    let fins_hs = build_fins_tcp_handshake_frame(2);
    let fins_read = build_fins_core_command("D100", 10, false, None)?;
    let fins_multi = build_fins_multiple_read_command(&["D100".into(), "CIO10.0".into(), "W20.1".into()])?;
    let hostlink_pkt = pack_omron_hostlink_serial(0, &fins_multi);
    println!("  [Omron FINS] 20B TCP Handshake: {}", bytes_to_hex(&fins_hs));
    println!("  [Omron FINS] 01 01 Continuous Read (D100 x 10): {}", bytes_to_hex(&fins_read));
    println!("  [Omron FINS] 01 04 Multiple Memory Read: {}", bytes_to_hex(&fins_multi));
    println!("  [Omron HostLink] Serial @00FA... Packet: {}", String::from_utf8_lossy(&hostlink_pkt).trim());

    println!("================================================================================");

    if serve_mode {
        let addr = SocketAddr::from(([0, 0, 0, 0], web_port));
        let listener = TcpListener::bind(addr).await?;
        println!("\n  🚀 Interactive Workbench Web Server & REST API running at: http://127.0.0.1:{}/", web_port);
        println!("  支援連線真實實體設備 (Real Device via TCP/Socket) 與虛擬回環模擬。");
        println!("  Open http://localhost:{} in any browser to test.", web_port);
        println!("  Press Ctrl+C to terminate server.\n");

        let html_content = include_str!("../../../driver_workbench_demo.html");
        let html_bytes = Arc::new(html_content.as_bytes().to_vec());
        let shared_driver = Arc::new(Mutex::new(UniversalPlcDriver::new(DriverConfig::default())));

        while let Ok((stream, _)) = listener.accept().await {
            let html = html_bytes.clone();
            let drv = shared_driver.clone();
            tokio::spawn(async move {
                handle_http_client(stream, drv, html).await;
            });
        }
    } else {
        println!("Tip: Run with `cargo run -p driver-workbench -- --serve` to launch the interactive web GUI at http://localhost:8080");
        println!("Tip: Run with `cargo run -p driver-workbench -- --help` to view real PLC hardware connection commands.");
    }

    Ok(())
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ConnectRequest {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub kind: serde_json::Value,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_station")]
    pub station: u8,
    #[serde(default)]
    pub rack: u8,
    #[serde(default = "default_slot")]
    pub slot: u8,
    #[serde(default)]
    pub byte_order: serde_json::Value,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub loopback_emulation: bool,
    #[serde(default)]
    pub plc_family: Option<String>,
}
fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 502 }
fn default_station() -> u8 { 1 }
fn default_slot() -> u8 { 1 }
fn default_timeout() -> u64 { 1000 }

fn resolve_driver_kind(val: &serde_json::Value) -> DriverKind {
    if let Some(s) = val.as_str() {
        if let Ok(k) = serde_json::from_str::<DriverKind>(&format!("\"{}\"", s)) {
            return k;
        }
        let clean = s.to_lowercase().replace(['_', '-', ' ', '(', ')', '/'], "");
        let catalog = list_driver_catalog();
        for item in &catalog {
            let item_k = format!("{:?}", item.kind).to_lowercase().replace(['_', '-', ' '], "");
            let cls_k = item.csharp_class.to_lowercase().replace(['_', '-', ' ', '(', ')', '/'], "");
            if item_k == clean || cls_k.contains(&clean) || clean.contains(&item_k) {
                return item.kind;
            }
        }
    }
    DriverKind::ModbusTcp
}

fn resolve_byte_order(val: &serde_json::Value) -> ByteOrder {
    if let Some(s) = val.as_str() {
        match s.to_uppercase().as_str() {
            "BADC" => ByteOrder::BADC,
            "CDAB" => ByteOrder::CDAB,
            "DCBA" => ByteOrder::DCBA,
            _ => ByteOrder::ABCD,
        }
    } else {
        ByteOrder::ABCD
    }
}

async fn handle_http_client(
    mut stream: tokio::net::TcpStream,
    driver: Arc<Mutex<UniversalPlcDriver>>,
    html_bytes: Arc<Vec<u8>>,
) {
    let mut buf = Vec::new();
    let mut temp = [0u8; 4096];
    let mut content_length = None;
    let mut header_len = None;

    loop {
        let n = match stream.read(&mut temp).await {
            Ok(n) if n > 0 => n,
            _ => break,
        };
        buf.extend_from_slice(&temp[..n]);

        if header_len.is_none() {
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                header_len = Some(pos + 4);
                let header_str = String::from_utf8_lossy(&buf[..pos]);
                for line in header_str.lines() {
                    if let Some(val) = line.to_lowercase().strip_prefix("content-length:") {
                        if let Ok(cl) = val.trim().parse::<usize>() {
                            content_length = Some(cl);
                        }
                    }
                }
            }
        }

        if let (Some(hlen), Some(clen)) = (header_len, content_length) {
            if buf.len() >= hlen + clen {
                break;
            }
        } else if header_len.is_some() && content_length.is_none() {
            break;
        }
    }

    if buf.is_empty() {
        return;
    }

    let hlen = header_len.unwrap_or(buf.len());
    let header_part = String::from_utf8_lossy(&buf[..hlen]);
    let body_part = if buf.len() > hlen {
        String::from_utf8_lossy(&buf[hlen..]).to_string()
    } else {
        String::new()
    };

    let mut lines = header_part.lines();
    let first_line = lines.next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];
    let body = body_part.trim();

    if method == "OPTIONS" {
        let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Accept, Origin\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(resp.as_bytes()).await;
        return;
    }

    if method == "GET" && (path == "/" || path == "/index.html") {
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
            html_bytes.len()
        );
        let _ = stream.write_all(resp.as_bytes()).await;
        let _ = stream.write_all(&html_bytes).await;
        return;
    }

    if method == "GET" && path == "/api/catalog" {
        let catalog = list_driver_catalog();
        let json = serde_json::to_string(&catalog).unwrap_or_default();
        send_json_response(&mut stream, 200, &json).await;
        return;
    }

    if method == "POST" && path == "/api/connect" {
        let resp_json = match serde_json::from_str::<ConnectRequest>(body) {
            Ok(req) => {
                let kind = resolve_driver_kind(&req.kind);
                let byte_order = resolve_byte_order(&req.byte_order);
                let cfg = DriverConfig {
                    id: req.id.unwrap_or_else(|| "web_plc".into()),
                    kind,
                    host: req.host,
                    port: req.port,
                    station: req.station,
                    rack: req.rack,
                    slot: req.slot,
                    byte_order,
                    timeout_ms: req.timeout_ms,
                    scan_interval_ms: 50,
                    loopback_emulation: req.loopback_emulation,
                    plc_family: plc_common::Family::InovanceEasyTcp,
                    serial: None,
                };
                let mut drv = driver.lock().await;
                match drv.connect(cfg) {
                    Ok(trace) => serde_json::json!({ "success": true, "trace": trace }).to_string(),
                    Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }).to_string(),
                }
            }
            Err(e) => serde_json::json!({ "success": false, "error": format!("JSON 解析錯誤: {}", e) }).to_string(),
        };
        send_json_response(&mut stream, 200, &resp_json).await;
        return;
    }

    if method == "POST" && path == "/api/disconnect" {
        let mut drv = driver.lock().await;
        let _ = drv.disconnect();
        send_json_response(&mut stream, 200, r#"{"success":true}"#).await;
        return;
    }

    if method == "POST" && path == "/api/read_single" {
        #[derive(serde::Deserialize)]
        struct ReadReq { address: String, data_type: String }
        let resp_json = match serde_json::from_str::<ReadReq>(body) {
            Ok(req) => {
                let mut drv = driver.lock().await;
                match drv.read_single(&req.address, &req.data_type) {
                    Ok(res) => serde_json::json!({ "success": true, "result": res }).to_string(),
                    Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }).to_string(),
                }
            }
            Err(e) => serde_json::json!({ "success": false, "error": format!("JSON 解析錯誤: {}", e) }).to_string(),
        };
        send_json_response(&mut stream, 200, &resp_json).await;
        return;
    }

    if method == "POST" && path == "/api/write_single" {
        #[derive(serde::Deserialize)]
        struct WriteReq { address: String, data_type: String, value: serde_json::Value }
        let resp_json = match serde_json::from_str::<WriteReq>(body) {
            Ok(req) => {
                let val = match req.value {
                    serde_json::Value::Bool(b) => ScalarValue::Bool(b),
                    serde_json::Value::Number(n) => ScalarValue::Number(n.as_f64().unwrap_or(0.0)),
                    serde_json::Value::String(s) => ScalarValue::Text(s),
                    _ => ScalarValue::Number(0.0),
                };
                let mut drv = driver.lock().await;
                match drv.write_single(&req.address, &req.data_type, val) {
                    Ok(ack) => serde_json::json!({ "success": true, "ack": ack }).to_string(),
                    Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }).to_string(),
                }
            }
            Err(e) => serde_json::json!({ "success": false, "error": format!("JSON 解析錯誤: {}", e) }).to_string(),
        };
        send_json_response(&mut stream, 200, &resp_json).await;
        return;
    }

    if method == "POST" && path == "/api/read_continuous" {
        #[derive(serde::Deserialize)]
        struct ContReq { start_address: String, length: u16, data_type: String }
        let resp_json = match serde_json::from_str::<ContReq>(body) {
            Ok(req) => {
                let mut drv = driver.lock().await;
                match drv.read_continuous(&req.start_address, req.length, &req.data_type) {
                    Ok(res) => serde_json::json!({ "success": true, "result": res }).to_string(),
                    Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }).to_string(),
                }
            }
            Err(e) => serde_json::json!({ "success": false, "error": format!("JSON 解析錯誤: {}", e) }).to_string(),
        };
        send_json_response(&mut stream, 200, &resp_json).await;
        return;
    }

    if method == "POST" && path == "/api/write_continuous" {
        #[derive(serde::Deserialize)]
        struct ContWriteReq { start_address: String, data_type: String, values: Vec<serde_json::Value> }
        let resp_json = match serde_json::from_str::<ContWriteReq>(body) {
            Ok(req) => {
                let vals: Vec<ScalarValue> = req.values.into_iter().map(|v| match v {
                    serde_json::Value::Bool(b) => ScalarValue::Bool(b),
                    serde_json::Value::Number(n) => ScalarValue::Number(n.as_f64().unwrap_or(0.0)),
                    serde_json::Value::String(s) => ScalarValue::Text(s),
                    _ => ScalarValue::Number(0.0),
                }).collect();
                let mut drv = driver.lock().await;
                match drv.write_continuous(&req.start_address, &req.data_type, &vals) {
                    Ok(ack) => serde_json::json!({ "success": true, "ack": ack }).to_string(),
                    Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }).to_string(),
                }
            }
            Err(e) => serde_json::json!({ "success": false, "error": format!("JSON 解析錯誤: {}", e) }).to_string(),
        };
        send_json_response(&mut stream, 200, &resp_json).await;
        return;
    }

    if method == "POST" && path == "/api/ping_latency" {
        #[derive(serde::Deserialize)]
        struct LatReq { address: String, rounds: u32 }
        let resp_json = match serde_json::from_str::<LatReq>(body) {
            Ok(req) => {
                let mut drv = driver.lock().await;
                match drv.ping_latency(&req.address, req.rounds) {
                    Ok(res) => serde_json::json!({ "success": true, "report": res }).to_string(),
                    Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }).to_string(),
                }
            }
            Err(e) => serde_json::json!({ "success": false, "error": format!("JSON 解析錯誤: {}", e) }).to_string(),
        };
        send_json_response(&mut stream, 200, &resp_json).await;
        return;
    }

    if method == "POST" && path == "/api/stress_test" {
        let resp_json = match serde_json::from_str::<StressTestConfig>(body) {
            Ok(req) => {
                let mut drv = driver.lock().await;
                match drv.run_stress_test(&req) {
                    Ok(res) => serde_json::json!({ "success": true, "report": res }).to_string(),
                    Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }).to_string(),
                }
            }
            Err(e) => serde_json::json!({ "success": false, "error": format!("JSON 解析錯誤: {}", e) }).to_string(),
        };
        send_json_response(&mut stream, 200, &resp_json).await;
        return;
    }

    send_json_response(&mut stream, 404, r#"{"error":"Not Found"}"#).await;
}

async fn send_json_response(stream: &mut tokio::net::TcpStream, status: u16, json: &str) {
    let status_line = match status {
        200 => "200 OK",
        400 => "400 Bad Request",
        404 => "404 Not Found",
        _ => "500 Internal Server Error",
    };
    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json; charset=utf-8\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_line,
        json.len(),
        json
    );
    let _ = stream.write_all(resp.as_bytes()).await;
}
