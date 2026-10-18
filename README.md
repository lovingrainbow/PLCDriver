# PLCDriver — Industrial Automation PLC Driver Engine in Rust

本專案將主流工業通訊協定完整重構為 **100% 安全 Rust** (`#![forbid(unsafe_code)]`) 語言，並對接至 **ArgonGUI** 的 `mm-drivers`、`mm-core`（`DataService`）與 `src-tauri` IPC 層。

涵蓋 **PLC、Modbus、工業機器人 (Robot)、CNC 數控系統、智慧儀表 (Instrument)、專用扭力控制器與 RS-232 / RS-485 實體序列埠** 等 76 種工業硬體驅動通訊協議，並提供包含**單點讀寫（含 Read-back 驗證）、連續區塊讀寫、跨區隨機讀寫、RTT 通訊延遲量測與高併發壓力測試**的完整引擎與圖形測試台。

---

## 📁 專案架構目錄

```text
PLCDriver/
├── Cargo.toml                                 # Cargo Workspace 配置 (含 mm-drivers, driver-workbench, src-tauri)
├── README.md                                  # 專案架構與全硬體驅動手冊
├── USAGE.md                                   # API 使用指南與速查表
├── driver.md                                  # 原始 PLC 驅動需求文檔 (Phase 1)
├── driver2.md                                 # 全硬體盤點與轉譯文檔 (Phase 2)
├── driver3.md                                 # 序列埠與深度協定轉譯文檔 (Phase 3)
├── all_hardware_workbench.html                # 全硬體互動式測試工作台 (支援 76 種硬體驅動分類篩選)
├── driver_workbench_demo.html                 # 驅動測試工作台 (預設 HTML)
├── ui/
│   ├── all_hardware_workbench.html            # 供 Web Server 與 Tauri 視窗載入之頁面
│   └── driver_workbench_demo.html
├── crates/
│   ├── mm-drivers/                            # Rust 驅動核心庫 (lib.rs + serial_and_protocols.rs + 76 DriverKind（PLC 型號共 56 種家族，含 InovanceEasySerial 序列埠驅動）)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs                         # UniversalPlcDriver, PlcDriver, ByteTransform, Helper 報文封裝器
│   │   │   └── serial_and_protocols.rs        # RS-232/485 (tokio-serial) + 4大校驗碼 + FX/Inovance/Omron 專屬協議
│   │   └── tests/
│   │       ├── plc_driver_tests.rs            # 完整單元與各廠牌協定整合測試
│   │       ├── family_consistency.rs          # DriverKind 與 plc-common::Family 一致性
│   │       ├── tight_timeout.rs               # 緊湊逾時
│   │       ├── serial_rtu.rs                  # 序列埠 RTU（僅 pty 模擬）
│   │       └── no_machine_model.rs            # 依賴規則：不得依賴 machine-model
│   └── driver-workbench/                      # 驅動工作台執行檔 (CLI 測試 + 離線 Web 伺服器)
│       ├── Cargo.toml
│       └── src/
│           └── main.rs                        # 終端機壓測輸出與 HTTP 伺服器
└── src-tauri/                                 # Tauri 2.0 桌面應用程式與 IPC 橋接 (10 個 Commands)
    ├── Cargo.toml
    ├── tauri.conf.json                        # 跨平台視窗組態
    └── src/
        ├── lib.rs
        ├── main.rs
        └── driver_commands.rs                 # Tauri 2.0 Commands (driver_read_single, driver_connect 等)
```

---

## 🚀 支援硬體分類目錄

### 1. PLC 廠牌協定 (41 款)
* **三菱電機 Mitsubishi**：
  * `MelsecMcBinary` (QnA-3E 二進位，埠 5000，`DCBA`)
  * `MelsecMcAscii` (QnA-3E ASCII，埠 5000，`DCBA`)
  * `MelsecMcRBinary` (iQ-R 4 位元組位址 3E 幀，埠 5000，`DCBA`)
  * `MelsecA1EBinary` / `MelsecA1EAscii` (A-1E 幀，埠 5000，`DCBA`)
  * `MelsecA3CTcp` (QnA-3C 幀，埠 5000，`DCBA`)
  * `MelsecFxSerialTcp` (FX 編程口，支援 E00/E10 擴充指令與 0x37/0x38 位元強制，埠 5000，`DCBA`)
  * `MelsecFxLinksTcp` (FX 485 Computer Link，支援 BR/WR/BW/WW，埠 5000，`DCBA`)
  * `MelsecCipNet` (iQ-R EtherNet/IP，埠 44818，`DCBA`)
* **西門子 Siemens**：
  * `SiemensS7_1200` / `SiemensS7_1500` / `SiemensS7_300` / `SiemensS7_400` (ISO-on-TCP TPKT+COTP+S7，埠 102，`ABCD`)
  * `SiemensS7_200Smart` / `SiemensS7_200` (CP243，埠 102，`ABCD`)
  * `SiemensFetchWrite` (Fetch/Write 協定，埠 2000，`ABCD`)
  * `SiemensPpiTcp` (PPI 點對點協定 Over TCP，埠 2000，`ABCD`)
* **歐姆龍 Omron**：
  * `OmronFinsTcp` / `OmronFinsUdp` (FINS 節點通訊，支援 20B 握手與 01 04 隨機多點讀取，埠 9600，`ABCD`)
  * `OmronCipNet` (NJ/NX/NY 系列 EtherNet/IP 標籤變數讀寫，埠 44818，`DCBA`)
  * `OmronHostLinkTcp` / `OmronHostLinkCModeTcp` (HostLink 上位鏈路，支援 `@00FA...FCS*\r` 串列封裝，埠 9600，`ABCD`)
* **基恩士 Keyence**：
  * `KeyenceMcBinary` / `KeyenceMcAscii` (KV-5000/7000/8000 3E 幀，埠 5000，`DCBA`)
  * `KeyenceNanoTcp` (KV-Nano 上位鏈路指令 `RDS`/`WRS`，埠 8501，`DCBA`)
* **匯川技術 Inovance**：
  * `InovanceEasyTcp` / `InovanceH3uTcp` / `InovanceH5uTcp` / `InovanceAmTcp` (Easy/H3U/H5U/AM 系列，埠 502，`CDAB`，支援八進位 X/Y、SM/SD、R/B/S/T/C、MW/MX)
* **台達電子 Delta**：
  * `DeltaDvpTcp` / `DeltaAsTcp` / `DeltaSerialAsciiTcp` (DVP/AS300/AH500，埠 502，`CDAB`)
* **羅克韋爾 Allen-Bradley**：
  * `AllenBradleyCip` / `AllenBradleyMicroCip` (ControlLogix / Micro800 CIP，埠 44818，`DCBA`)
  * `AllenBradleySlcNet` / `AllenBradleyPccc` / `AllenBradleyDf1Tcp` (SLC500 / DF1，埠 44818，`DCBA`)
* **其他主流 PLC**：
  * `BeckhoffAds` (倍福 TwinCAT 2/3 ADS/TCP，埠 48898，`DCBA`)
  * `PanasonicMewtocolTcp` / `PanasonicMcBinary` (松下 FP Mewtocol / MC，埠 9094 / 5000，`DCBA`)
  * `FatekProgramTcp` (台灣永宏 FBS 編程口協定，埠 500，`ABCD`)
  * `LSisFastEnet` / `LSisCnetTcp` (韓國樂星 XGT FastEnet，埠 2004，`DCBA`)
  * `FujiSpbTcp` / `FujiSphNet` / `FujiCsType` (富士電機 FLEX-PC / MICREX-SX，埠 5000 / 507，`DCBA`)
  * `XinjeTcp` / `XinjeInternalNet` / `XinjeSerialTcp` (信捷電氣 XC/XD/XL，埠 502，`ABCD`)
  * `YokogawaLinkTcp` (橫河電機 FA-M3 LinkTcp，埠 12289，`ABCD`)
  * `YaskawaMemobusTcp` (安川電機 Memobus MP 系列，埠 9999，`ABCD`)
  * `GeSrtpTcp` (GE 通用電氣 PACSystems RX3i SRTP，埠 18245，`DCBA`)
  * `VigorTcp` (台灣豐煒 VB/VH 系列，埠 5000，`DCBA`)
  * `ToyotaToyoPuc` (豐田工機 TOYOPUC PC10G，埠 1025，`DCBA`)
  * `CimonHmiTcp` (西蒙 CIMON-PLC 協定，埠 10260，`ABCD`)

### 2. Modbus 標準通訊 (4 款)
* `ModbusTcp` (標準 Modbus TCP，功能碼 01/02/03/04/05/06/10，埠 502，`ABCD`)
* `ModbusRtuOverTcp` (含 CRC-16 校驗，埠 502，`ABCD`)
* `ModbusAsciiOverTcp` (含 LRC 校驗，埠 502，`ABCD`)
* `ModbusUdp` (Modbus UDP 廣播/單播，埠 502，`ABCD`)

### 3. 工業機器人 (Robot 6 款)
* `RobotFanucNet` (FANUC 發那科機器人介面，埠 60008，`DCBA`，支援 `R[1]`, `DI[1]`, `DO[1]`)
* `RobotEstunTcp` (Estun 埃斯頓機器人 Modbus 協定，埠 502，`ABCD`)
* `RobotEfortTcp` (EFORT 埃夫特機器人，埠 8008，`DCBA`)
* `RobotKukaAvar` (KUKA 庫卡機器人 OpenShowVar / KukaAvarProxy，埠 7000，`ABCD`，支援 `$OV_PRO`, `$POS_ACT`)
* `RobotYaskawaYrc` (Yaskawa 安川 YRC1000 高速乙太網，埠 10040，`ABCD`)
* `RobotAbbWeb` (ABB 機器人 OmniCore / RobotWare Web API 橋接，埠 80，`ABCD`)

### 4. CNC 數控系統與儀表 (CNC / Instrument 5 款)
* `CncFanuc0i` (FANUC Series 0i/30i/31i Focas 乙太網，埠 8193，`ABCD`，支援主軸轉速、進給率、刀具號)
* `InstDlt645Tcp` (中國國家標準 DL/T 645-2007 智慧電表 Over TCP，埠 2000，`DCBA`，支援電壓/電流/電能)
* `InstDlt698Tcp` (DL/T 698.45 物聯網智慧電表，埠 2000，`DCBA`)
* `InstRkcTempTcp` (RKC 理化工業溫控器通訊協定，埠 4001，`ABCD`，支援 `M1` PV, `S1` SV)
* `InstCjt188Tcp` (CJ/T 188 水表/熱量表 Over TCP，埠 2000，`ABCD`)
* `KeyenceDlEn1` (Keyence 感測器通訊模組，埠 9004) / `KeyenceSr2000` (Keyence 工業讀碼器，埠 9000)

### 5. 工業專用扭力控制器
* `OpenProtocolTcp` (Atlas Copco 阿特拉斯馬達槍 OpenProtocol，埠 4545，`ABCD`，支援 `MID0001` 通訊握手、`MID0060` 扭力/角度讀取)

---

## 🛠️ 快速開始與驗證

### 1. 執行全硬體單元與報文整合測試
```bash
cargo test -p mm-drivers
```
測試已全數通過，涵蓋 CRC-16 計算、Endianness、通用位址解析、三菱/西門子/歐姆龍/松下/倍福/電表/開放協議真實封包驗證、以及 200 輪高頻壓測。

### 2. 執行命令列驅動性能基準測試
```bash
cargo run -p driver-workbench
```
自動列印全部支援硬體、進行單點/連續/隨機讀寫測試、RTT 延遲分析與 2,000 筆高頻壓測（實測吞吐量達 16,000+ ops/sec）。

### 3. 啟動互動式全硬體 Web 工作台
```bash
cargo run -p driver-workbench -- --serve
```
啟動後於瀏覽器開啟：[http://127.0.0.1:8080](http://127.0.0.1:8080)
亦可直接雙擊開啟專案目錄下的 [`all_hardware_workbench.html`](file:///mnt/c/Users/wydct/orca/projects/PLCDriver/all_hardware_workbench.html)，提供分類按鈕（全部、PLC、Modbus、機器人、CNC/儀表）與即時搜尋功能。
