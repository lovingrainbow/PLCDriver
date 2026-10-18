# Industrial Automation Driver Engine (`mm-drivers`) — API 使用指南 (USAGE.md)

本文件專為工程師與 AI Agent 設計，提供完整的 API 使用規範、架構說明、各廠牌位址語法對照表、前端 Tauri IPC 呼叫範例與基準測試說明。無需再翻閱源碼即可直接複製運行。

---

## 目錄 (Table of Contents)
1. [專案架構與依賴配置](#1-專案架構與依賴配置)
2. [核心抽象與型別系統](#2-核心抽象與型別系統)
3. [快速上手：初始化與連線](#3-快速上手初始化與連線)
4. [核心 API 實戰範例](#4-核心-api-實戰範例)
   - [4.1 單點讀取與 Read-back 驗證寫入](#41-單點讀取與-read-back-驗證寫入)
   - [4.2 連續區塊讀寫 (Continuous / Batch I/O)](#42-連續區塊讀寫-continuous--batch-io)
   - [4.3 隨機多點讀寫 (Scatter-Gather Random I/O)](#43-隨機多點讀寫-scatter-gather-random-io)
   - [4.4 通訊延遲與抖動分析 (Latency & Jitter Profiler)](#44-通訊延遲與抖動分析-latency--jitter-profiler)
   - [4.5 高並發壓力測試 (Stress Benchmark)](#45-高並發壓力測試-stress-benchmark)
   - [4.6 故障注入與模擬模式 (Simulation Modes)](#46-故障注入與模擬模式-simulation-modes)
   - [4.7 RS-232 / RS-485 實體序列埠與檢核碼模組](#47-rs-232--rs-485-實體序列埠與檢核碼模組)
5. [76 種工業硬體位址格式速查表 (Address Syntax Cheatsheet)](#5-76-種工業硬體位址格式速查表-address-syntax-cheatsheet)
6. [Tauri IPC 前端整合介面 (Frontend IPC Commands)](#6-tauri-ipc-前端整合介面-frontend-ipc-commands)
7. [CLI 效能測試與 Web 離線工作台](#7-cli-效能測試與-web-離線工作台)

---

## 1. 專案架構與依賴配置

本驅動引擎完全採用 **100% 安全 Rust** (`#![forbid(unsafe_code)]`) 打造，重寫並優化了工業自動化通訊協議核心。

### 模組結構
- [`crates/mm-drivers`](file:///mnt/c/Users/wydct/orca/projects/PLCDriver/crates/mm-drivers/src/lib.rs): 核心通訊庫，包含 `PlcDriver` Trait、`UniversalPlcDriver`、76 種驅動報文建構器（含匯川 Easy 序列埠 `InovanceEasySerial`）、`ByteTransform` 以及 `serial_and_protocols` 模組。
- 型別（`ScalarValue`、`Family` 即 `PlcFamily`）來自外部 crate `plc-common`（`../plc-common`），mm-drivers 以 `pub use` 重新匯出；PLCDriver 不再內含 `machine-model`。
- [`crates/driver-workbench`](file:///mnt/c/Users/wydct/orca/projects/PLCDriver/crates/driver-workbench/src/main.rs): 獨立 CLI 壓測工具與離線 HTTP Workbench 伺服器。
- [`src-tauri`](file:///mnt/c/Users/wydct/orca/projects/PLCDriver/src-tauri/src/driver_commands.rs): 提供桌面端與 Web GUI 呼叫的 Tauri IPC 接口。

### Cargo.toml 配置範例
在您的專案 `Cargo.toml` 加入：
```toml
[dependencies]
mm-drivers = { path = "../PLCDriver/crates/mm-drivers" }
plc-common = { path = "../plc-common" }
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.38", features = ["full"] }
```

---

## 2. 核心抽象與型別系統

### 2.1 位元組序 (ByteOrder)
工業標準 16/32/64-bit 位元組序定義：
| 列舉值 | 說明 | 適用設備範例 |
|:---|:---|:---|
| `ByteOrder::ABCD` | 大端序 (Big-Endian)，網路標準序 | Siemens S7, Modbus 預設, Omron FINS |
| `ByteOrder::BADC` | 單字高低位元組正常、雙字高低字反轉 | 特殊 Modbus 儀表、熱量表 |
| `ByteOrder::CDAB` | 單字位元組反轉、雙字正常 | 匯川 Inovance, 台達 Delta DVP/AS |
| `ByteOrder::DCBA` | 小端序 (Little-Endian)，x86 標準 | 三菱 Melsec, 基恩士 KV, CIP Net |

### 2.2 驅動種類 (DriverKind)
共涵蓋 76 種硬體分類（詳細清單見第 5 節），例如：
- `DriverKind::SiemensS7_1200`
- `DriverKind::MelsecMcBinary`
- `DriverKind::InovanceEasyTcp`
- `DriverKind::OmronFinsTcp`
- `DriverKind::ModbusTcp`
- `DriverKind::RobotFanucNet`
- `DriverKind::CncFanuc0i`
- `DriverKind::Simulator` (內建高擬真軟體 PLC)

### 2.3 統一數值容器 (ScalarValue)
```rust
pub enum ScalarValue {
    Bool(bool),
    Number(f64),
    Text(String),
}
```

---

## 3. 快速上手：初始化與連線

驅動支援兩種工作模式：
1. **實體硬體連線**：連線真實 PLC / 儀表 TCP 埠或 RS-232/485 序列埠。
2. **零配置迴路模擬 (Loopback Emulation)**：`loopback_emulation = true` 時，即使沒有接上硬體設備，亦會在本地記憶體空間建立虛擬映射，自動回傳真實協定的 Hex 封包。

```rust
use mm_drivers::{DriverConfig, DriverKind, ByteOrder, UniversalPlcDriver, PlcDriver};
use plc_common::Family as PlcFamily;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 建立匯川 (Inovance) PLC 設定
    let config = DriverConfig {
        id: "plc_station_1".to_string(),
        kind: DriverKind::InovanceEasyTcp,
        host: "192.168.1.50".to_string(),
        port: 502,
        station: 1,
        rack: 0,
        slot: 1,
        byte_order: ByteOrder::CDAB,
        timeout_ms: 1000,
        scan_interval_ms: 50,
        loopback_emulation: true, // 無硬體時設為 true 可立即在本機測試
        plc_family: PlcFamily::InovanceEasyTcp,
        serial: None,
    };

    // 實例化驅動
    let mut driver = UniversalPlcDriver::new(config.clone());

    // 建立連線
    let trace = driver.connect(config)?;
    println!("連線成功！耗時: {} µs, 握手報文: {}", trace.elapsed_us, trace.tx_hex);

    Ok(())
}
```

---

## 4. 核心 API 實戰範例

### 4.1 單點讀取與 Read-back 驗證寫入
工業控制中，單純寫入是不安全的。`write_single` 自動在寫入後發起二次讀取驗證（Read-back），並在 `WriteAck` 回報 `readback_matched`。

```rust
use plc_common::ScalarValue;
use mm_drivers::{UniversalPlcDriver, PlcDriver};

// 1. 讀取暫存器 D120 (16-bit 整數)
let read_res = driver.read_single("D120", "int16")?;
println!("讀取 D120: {:?} (品質: {:?}, 耗時: {} µs)", 
    read_res.value, read_res.quality, read_res.trace.elapsed_us);

// 2. 寫入暫存器 D120 並自動執行 Read-back 驗證
let ack = driver.write_single("D120", "int16", ScalarValue::Number(2048.0))?;
println!("寫入 D120 = 2048: 驗證是否相符: {}, 實際回讀值: {:?}", 
    ack.readback_matched, ack.readback_value);
assert!(ack.readback_matched);

// 3. 讀寫位元 (Bit) 元件 (例如 M100 或 X000)
driver.write_single("M100", "bool", ScalarValue::Bool(true))?;
let bit_res = driver.read_single("M100", "bool")?;
assert_eq!(bit_res.value, ScalarValue::Bool(true));

// 4. 讀寫 32 位元浮點數 (Float32)
driver.write_single("D150", "float32", ScalarValue::Number(75.5))?;
let f_res = driver.read_single("D150", "float32")?;
println!("D150 浮點數: {:?}", f_res.value);
```

### 4.2 連續區塊讀寫 (Continuous / Batch I/O)
在採集多個連續暫存器時，連續讀取可降低 90% 以上的網路來回時間。

```rust
// 連續讀取 D100 開始的 10 個 Word (D100 ~ D109)
let batch_read = driver.read_continuous("D100", 10, "int16")?;
println!("連續讀取數值陣列: {:?}", batch_read.values);
println!("原始 Hex 位元組流: {}", batch_read.raw_bytes_hex);

// 連續批次寫入
let new_values: Vec<ScalarValue> = (0..10)
    .map(|i| ScalarValue::Number(f64::from(100 + i * 10)))
    .collect();
let batch_ack = driver.write_continuous("D100", "int16", &new_values)?;
println!("批次寫入成功: {} 耗時: {} µs", batch_ack.address, batch_ack.trace.elapsed_us);
```

### 4.3 隨機多點讀寫 (Scatter-Gather Random I/O)
適用於畫面監視器同時抓取分散在不同區塊的標籤。

```rust
use mm_drivers::{RandomReadItem, RandomWriteItem};

// 定義散佈在不同區塊的暫存器與位元
let items = vec![
    RandomReadItem { address: "D100".into(), data_type: "int16".into() },
    RandomReadItem { address: "D150".into(), data_type: "float32".into() },
    RandomReadItem { address: "M104".into(), data_type: "bool".into() },
    RandomReadItem { address: "X010".into(), data_type: "bool".into() },
];

let scatter_res = driver.read_random(&items)?;
for item in &scatter_res.items {
    println!("點位: {} -> 數值: {:?}, 耗時: {} µs", 
        item.address, item.value, item.trace.elapsed_us);
}
println!("總花費通訊時間: {} µs", scatter_res.total_elapsed_us);

// 隨機多點批次寫入
let write_items = vec![
    RandomWriteItem { address: "D100".into(), data_type: "int16".into(), value: ScalarValue::Number(999.0) },
    RandomWriteItem { address: "M104".into(), data_type: "bool".into(), value: ScalarValue::Bool(false) },
];
let write_acks = driver.write_random(&write_items)?;
println!("成功寫入 {} 個離散點位", write_acks.len());
```

### 4.4 通訊延遲與抖動分析 (Latency & Jitter Profiler)
評估現場工業網路品質與封包穩定性。

```rust
// 對 D150 進行 100 輪 RTT 延遲探測
let report = driver.ping_latency("D150", 100)?;
println!("=== 通訊延遲分析報告 ===");
println!("測試輪數: {} (成功: {}, 失敗: {})", report.rounds, report.success_count, report.fail_count);
println!("延遲統計 (µs): 最小={}, 平均={}, 最大={}", report.min_us, report.avg_us, report.max_us);
println!("百分位數 (µs): P50={}, P95={}, P99={}", report.p50_us, report.p95_us, report.p99_us);
println!("網路抖動 (Jitter): {} µs", report.jitter_us);
```

### 4.5 高並發壓力測試 (Stress Benchmark)
驗證百萬級讀寫負載與驅動極限吞吐量。

```rust
use mm_drivers::{StressTestConfig, StressMode};

let stress_cfg = StressTestConfig {
    mode: StressMode::ContinuousRead, // 支援 SingleRead, SingleWrite, ContinuousRead, ContinuousWrite, RandomRead
    address: "D100".into(),
    length: 64,        // 每次讀取 64 個 Word
    concurrency: 1,
    iterations: 5000,  // 執行 5,000 次操作
};

let bench = driver.run_stress_test(&stress_cfg)?;
println!("=== 壓力測試結果 ===");
println!("總執行次數: {} (成功={}, 失敗={})", bench.total_ops, bench.success_ops, bench.failed_ops);
println!("總耗時: {:.2} ms", bench.total_duration_ms);
println!("吞吐量: {:.0} 操作/秒 (Throughput)", bench.throughput_ops_per_sec);
println!("頻寬: {:.1} KB/秒 (Bandwidth)", bench.throughput_kb_per_sec);
println!("P95 延遲: {} µs, P99 延遲: {} µs", bench.p95_us, bench.p99_us);
```

### 4.6 故障注入與模擬模式 (Simulation Modes)
內建於各類測試環境中，用於模擬現場異常：
```rust
use mm_drivers::SimMode;

// 1. 模擬感測器故障 (數值亂跳/品質不良)
driver.set_simulation(SimMode::BadSensor)?;

// 2. 模擬網路通訊中斷
driver.set_simulation(SimMode::UncertainDisconnect)?;

// 3. 模擬 PLC 寫入被鎖死，導致 Readback 不一致報警
driver.set_simulation(SimMode::ReadbackFail)?;

// 4. 恢復正常通訊狀態
driver.set_simulation(SimMode::Good)?;
```

### 4.7 RS-232 / RS-485 實體序列埠與檢核碼模組
在 [`serial_and_protocols`](file:///mnt/c/Users/wydct/orca/projects/PLCDriver/crates/mm-drivers/src/serial_and_protocols.rs) 模組中，提供非同步序列埠建立與工業檢核碼運算：

```rust
use mm_drivers::{
    SerialPortConfig, SerialParity, open_tokio_serial_builder,
    checksum::{crc16_modbus, lrc_byte, melsec_sum_check_ascii, omron_fcs_ascii},
    inovance::{translate_inovance_address, InovanceSeries, build_inovance_serial_read_frame},
    melsec_fx::{build_fx_serial_read_frame, build_fx_links_frame},
    omron::{build_fins_core_command, build_fins_multiple_read_command, pack_omron_hostlink_serial},
};

// 1. 設定 RS-485 參數 (例如 19200-8-E-1)
let serial_cfg = SerialPortConfig {
    port_name: "COM3".into(), // 或 Linux: "/dev/ttyUSB0"
    baud_rate: 19200,
    data_bits: 8,
    parity: SerialParity::Even,
    stop_bits: 1,
    rs485_rts_enable: true,
    inter_frame_silence_ms: 10,
};
let builder = open_tokio_serial_builder(&serial_cfg);

// 2. 匯川全系列軟元件映射與報文生成 (Easy / H3U / H5U / AM)
let (is_bit, modbus_addr) = translate_inovance_address(InovanceSeries::H3U, "SD0")?;
let (addr, rtu_frame) = build_inovance_serial_read_frame(InovanceSeries::H3U, 1, "D100", 2, false)?;

// 3. 三菱 FX 編程口 (支援 FX3U E00 擴充指令) 與 485 Computer Link (BR/WR)
let fx_frame = build_fx_serial_read_frame("D100", 2, true)?;
let links_frame = build_fx_links_frame(0, "D100", 1, None, true)?;

// 4. 歐姆龍 FINS 隨機多點讀取與 HostLink 串列封裝
let fins_multi = build_fins_multiple_read_command(&["D100".into(), "CIO10.0".into()])?;
let hostlink_pkt = pack_omron_hostlink_serial(0, &fins_multi);
```

---

## 5. 76 種工業硬體位址格式速查表 (Address Syntax Cheatsheet)

本驅動內建通用的語法分析器 [`parse_universal_plc_address`](file:///mnt/c/Users/wydct/orca/projects/PLCDriver/crates/mm-drivers/src/lib.rs#L735-L782) 與各廠牌協議封包組裝器 [`build_protocol_frame`](file:///mnt/c/Users/wydct/orca/projects/PLCDriver/crates/mm-drivers/src/lib.rs#L785-L975)。

| 類別 | 硬體 / 廠牌 | DriverKind 列舉 | 預設 Port | 預設位元組序 | 位址語法範例與說明 |
|:---|:---|:---|:---:|:---:|:---|
| **西門子** | S7-1200 / 1500 | `SiemensS7_1200`, `SiemensS7_1500` | 102 | `ABCD` | `DB1.DBW0` (Word), `DB1.DBD4` (DWord/Float), `DB1.DBX0.1` (Bit), `M100`, `I0.0`, `Q0.0` |
| | S7-300 / 400 | `SiemensS7_300`, `SiemensS7_400` | 102 | `ABCD` | `DB1.DBW0`, `M100`, `I0.0`, `Q0.0` |
| | S7-200 SMART | `SiemensS7_200Smart` | 102 | `ABCD` | `V100` (對應 DB1.DBW100), `M100`, `I0.0`, `Q0.0` |
| | Fetch/Write / PPI | `SiemensFetchWrite`, `SiemensPpiTcp` | 2000 | `ABCD` | `DB1.DBW0`, `V100`, `M10` |
| **三菱** | iQ-R 系列 (3E) | `MelsecMcRBinary` | 5000 | `DCBA` | `D100`, `M100`, `ZR100` (32位元位址), `W100`, `X010` (8進制), `Y010` |
| | Q/L 系列 (3E Binary) | `MelsecMcBinary` | 5000 | `DCBA` | `D100`, `M100`, `X010` (8進制), `Y010`, `W100`, `R100`, `B10` |
| | Q/L 系列 (3E ASCII) | `MelsecMcAscii` | 5000 | `DCBA` | 同上，ASCII 編碼通訊格式 |
| | A-1E / FX 串列口 | `MelsecA1EBinary`, `MelsecFxSerialTcp` | 5000 | `DCBA` | `D100`, `M100`, `X10`, `Y10`, `S10` |
| | iQ-R CIP Tag | `MelsecCipNet` | 44818 | `DCBA` | `Global_Counter`, `Station_Status` |
| **Modbus** | Modbus TCP | `ModbusTcp` | 502 | `ABCD` | `100` (保持暫存器 0x03), `x=2;100` (輸入暫存器 0x04), `x=1;100` (線圈 0x01) |
| | Modbus RTU / ASCII | `ModbusRtuOverTcp`, `ModbusAsciiOverTcp` | 502 | `ABCD` | `s=2;100` (`s=站號;暫存器位址`)，自動附帶 CRC-16 / LRC 校驗 |
| **歐姆龍** | FINS TCP / UDP | `OmronFinsTcp`, `OmronFinsUdp` | 9600 | `ABCD` | `D100` (DM區), `CIO10.0` (CIO位元), `W10.0` (WR區), `H20.0` (HR區), `A100` |
| | NJ / NX / NY CIP | `OmronCipNet` | 44818 | `DCBA` | `Recipe_Temp`, `Motor_Run`, `D100` |
| | HostLink | `OmronHostLinkTcp`, `OmronHostLinkCModeTcp` | 9600 | `ABCD` | `D100`, `CIO10`, `W10`, `DM100` |
| **基恩士** | KV-5000/7000/8000 | `KeyenceMcBinary`, `KeyenceMcAscii` | 5000 | `DCBA` | `DM100`, `MR100`, `R100`, `EM100`, `LR100` |
| | KV Nano (上位鏈路) | `KeyenceNanoTcp` | 8501 | `DCBA` | `DM100`, `MR100`, `R000` |
| | 感測器 / 讀碼器 | `KeyenceDlEn1`, `KeyenceSr2000` | 9004 / 9000 | `DCBA` | `M0`, `SR,00,000`, `LON`, `LOFF` |
| **匯川** | Easy / H3U / H5U | `InovanceEasyTcp`, `InovanceH3uTcp`, `InovanceH5uTcp` | 502 | `CDAB` | `D100`, `M100`, `X010` (8進制), `Y010`, `SM0`, `SD0`, `R100`, `B10` |
| | AM400 / AM600 | `InovanceAmTcp` | 502 | `CDAB` | `MW100`, `MX10.0`, `IW0`, `QW0` |
| **台達** | DVP / AS 系列 | `DeltaDvpTcp`, `DeltaAsTcp` | 502 | `CDAB` | `D100`, `M100`, `X10`, `Y10`, `X0.0`, `Y0.0` |
| **羅克韋爾** | ControlLogix / CompactLogix | `AllenBradleyCip` | 44818 | `DCBA` | `Program:Main.HeaterPV`, `Global_Speed`, `D100` |
| | SLC500 / MicroLogix | `AllenBradleySlcNet`, `AllenBradleyPccc` | 44818 | `DCBA` | `N7:0` (整數), `B3:0/1` (位元), `F8:0` (浮點數) |
| **倍福** | TwinCAT 2/3 (ADS) | `BeckhoffAds` | 48898 | `DCBA` | `MAIN.nCount`, `GVL.bStart`, `D100` |
| **松下** | FP 系列 Mewtocol | `PanasonicMewtocolTcp` | 9094 | `DCBA` | `DT100`, `R10`, `X0`, `Y0` |
| **永宏** | FBS 系列 | `FatekProgramTcp` | 500 | `ABCD` | `D100`, `R100`, `M100`, `X0`, `Y0` |
| **樂星** | XGK / XGB 系列 | `LSisFastEnet`, `LSisCnetTcp` | 2004 | `DCBA` | `DW100`, `MW100`, `MX100` |
| **富士** | FLEX-PC / MICREX-SX | `FujiSpbTcp`, `FujiSphNet` | 5000 / 507 | `DCBA` | `D100`, `R100`, `M1.100` |
| **信捷** | XC / XD / XG 系列 | `XinjeTcp`, `XinjeInternalNet` | 502 | `ABCD` | `D100`, `M100`, `HD100`, `HM100` |
| **橫河** | FA-M3 系列 | `YokogawaLinkTcp` | 12289 | `ABCD` | `D100`, `B100`, `I10`, `M10` |
| **安川** | MP2000 / MP3000 | `YaskawaMemobusTcp` | 9999 | `ABCD` | `D100`, `M100`, `MW100` |
| **GE** | PACSystems RX3i | `GeSrtpTcp` | 18245 | `DCBA` | `R100`, `AI10`, `AQ10`, `M10`, `I10`, `Q10` |
| **機器人** | FANUC / KUKA / Yaskawa / ABB | `RobotFanucNet`, `RobotKukaAvar`, `RobotYaskawaYrc`, `RobotAbbWeb` | 各異 | `DCBA` / `ABCD` | `R[1]`, `DI[1]`, `DO[1]`, `$OV_PRO`, `$POS_ACT`, `rapid_var` |
| **CNC/儀表** | FANUC 0i CNC / DL/T 645 電表 | `CncFanuc0i`, `InstDlt645Tcp` | 8193 / 2000 | `ABCD` / `DCBA` | `SpindleSpeed`, `FeedRate`, `02-01-01-00` (A相電壓) |
| **扭力槍** | Atlas Copco | `OpenProtocolTcp` | 4545 | `ABCD` | `MID0001` (啟動通訊), `MID0060` (讀取擰緊結果) |

---

## 6. Tauri IPC 前端整合介面 (Frontend IPC Commands)

在桌面端環境中，後端在 [`src-tauri/src/driver_commands.rs`](file:///mnt/c/Users/wydct/orca/projects/PLCDriver/src-tauri/src/driver_commands.rs) 註冊了核心命令。前端 Vue / React / Svelte 均可透過 `@tauri-apps/api/core` 呼叫：

| Tauri Command 名稱 | 傳入參數 | 回傳型別 | 說明 |
|:---|:---|:---|:---|
| `driver_list_catalog` | 無 | `Vec<DriverCatalogItem>` | 列出 76 種支援硬體資訊與範例 |
| `driver_connect` | `config: DriverConfig` | `Result<DriverTrace, String>` | 建立硬體或模擬連線 |
| `driver_disconnect` | 無 | `Result<(), String>` | 斷開連線 |
| `driver_read_single` | `address: string, data_type: string` | `Result<SingleReadResult, String>` | 單點讀取暫存器或位元 |
| `driver_write_single` | `address: string, data_type: string, value: any` | `Result<WriteAck, String>` | 寫入並執行回讀驗證 |
| `driver_read_continuous` | `start_address: string, length: u16, data_type: string` | `Result<ContinuousReadResult, String>` | 連續區塊讀取 |
| `driver_write_continuous` | `start_address: string, data_type: string, values: any[]` | `Result<WriteAck, String>` | 連續區塊寫入 |
| `driver_read_random` | `items: RandomReadItem[]` | `Result<RandomReadResult, String>` | 隨機多點讀取 |
| `driver_write_random` | `items: RandomWriteItem[]` | `Result<WriteAck[], String>` | 隨機多點寫入 |
| `driver_ping_latency` | `address: string, rounds: u32` | `Result<LatencyReport, String>` | 通訊延遲與抖動測試 |
| `driver_stress_test` | `config: StressTestConfig` | `Result<StressTestReport, String>` | 50,000 次級別極限壓力測試 |

### 前端 TypeScript 呼叫範例
```typescript
import { invoke } from '@tauri-apps/api/core';

// 1. 連線至三菱 PLC
const connectResult = await invoke('driver_connect', {
  config: {
    id: "melsec_q",
    kind: "melsec_mc_binary",
    host: "192.168.1.10",
    port: 5000,
    station: 0,
    rack: 0,
    slot: 0,
    byte_order: "DCBA",
    timeout_ms: 1000,
    scan_interval_ms: 50,
    loopback_emulation: false,
    plc_family: "MitsubishiMelsec"
  }
});

// 2. 單點讀取
const data = await invoke('driver_read_single', {
  address: "D100",
  dataType: "int16"
});
console.log("D100 =", data.value, "耗時:", data.trace.elapsed_us, "µs");

// 3. 連續讀取 10 個點
const batch = await invoke('driver_read_continuous', {
  startAddress: "D100",
  length: 10,
  dataType: "int16"
});
console.log("連續數值陣列:", batch.values);
```

---

## 7. CLI 實體/虛擬測試與 Web 互動工作台

專案附帶獨立工具 [`driver-workbench`](file:///mnt/c/Users/wydct/orca/projects/PLCDriver/crates/driver-workbench/src/main.rs)，**完全支援真實實體設備直連與無硬體回環模擬兩種模式**：

### 7.1 為何預設為「虛擬站點 (Loopback Emulation)」？
- 預設模式（`loopback_emulation = true`）是為了讓開發者在**手邊沒有實體 PLC 或儀表硬體**時，依然能立即測試 76 種驅動協議的封包組裝、位址解析、連續讀寫以及並發壓測效能。
- **取消模擬即可直連真實設備**：底層通訊核心具備原生 `TcpStream` 與 `tokio-serial` 通訊能力，只要切換為真實硬體模式，便會直接對目標 IP/Port 或序列埠發起連線與雙向數據傳輸。

### 7.2 CLI 終端命令列直接連線實體 PLC
可透過命令列參數直接對廠房中的真實硬體發送讀寫指令：

```bash
# 1. 預設本機回環模擬基準測試
cargo run -p driver-workbench

# 2. 查看所有連線參數
cargo run -p driver-workbench -- --help

# 3. 連接真實實體設備 (Real Hardware) 執行單點讀取
cargo run -p driver-workbench -- --real --ip 192.168.1.10 --port 502 --kind modbus_tcp --read D100 --type int16

# 4. 連接真實實體設備執行單點寫入與回讀驗證
cargo run -p driver-workbench -- --real --ip 192.168.1.10 --port 502 --kind modbus_tcp --write D100=3888 --type int16

# 5. 連接真實實體設備執行連續區塊讀取 (Batch Read)
cargo run -p driver-workbench -- --real --ip 192.168.1.10 --port 502 --kind modbus_tcp --read D100 --count 16
```
> [!NOTE]
> 在 `--real` 模式下，若目標設備未開機或網路中斷，系統會回報真實的網路錯誤（如 `Connection refused` 或 `Connection timed out`），絕不假造成功。

### 7.3 啟動 Web 視覺化工作台 (整合 REST API 後端)
```bash
cargo run -p driver-workbench -- --serve --web-port 8080
```
在瀏覽器開啟 [http://127.0.0.1:8080](http://127.0.0.1:8080)：
- **連線模式切換**：
  - **勾選「本機回環模擬」**：進入無硬體展示模式，使用記憶體字典與報文演算法。
  - **取消勾選「本機回環模擬」**：進入 **`🟢 真實硬體直連模式`**，點擊「連線」時 Web 介面會透過後端 REST API 直接對輸入的實體 IP:Port（如 `192.168.1.10:502`）建立真實 Socket 連線，所有單點讀取、寫入與連續區塊監視表皆為真實硬體的真實暫存器數據！
- **即時協定報文監視器**：即時顯示真實發送 TX 與接收 RX 二進位報文與真實微秒級延遲。
- **序列埠與深度協議工具箱 (Tab 6)**：提供檢核碼計算器、匯川軟元件位址轉譯、三菱 FX 編程口/485BD、歐姆龍 FINS/HostLink 即時報文產生器。

