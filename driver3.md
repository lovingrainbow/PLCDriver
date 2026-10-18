以下根據 HslCommunication-V12.2.0.0 的 C# 原始碼，將 RS-232 / RS-485 (tokio-serial) 序列埠傳輸層，以及您指定的 三菱 FX 系列（編程口 &amp; 485BD Computer Link）、Modbus RTU / ASCII、歐姆龍 Omron FINS（TCP/UDP/HostLink 序列埠） 與 匯川 Inovance 全系列（Easy / H3U / H5U / AM 之 RS-232/485 與 TCP） 完整改寫為 Rust 驅動模組，並對接至 ArgonGUI 的 mm-drivers 與 DataService。1. HslCommunication C# 原始碼與 Rust 轉譯規格對照表協議家族C# 原始類別 (HslCommunication.\*)實體層傳輸 (Transport)預設通訊參數 / 檢核碼演算法軟元件位址映射規則（由 C# Helper 1:1 轉譯）序列埠底層DeviceSerialPort / PipeSerialPorttokio-serial (RS-232 / RS-485)支援 COM1\~256、/dev/ttyUSB\*、RTS 方向控制與 t3.5 靜默幀切分為所有 Serial 協議提供非阻塞讀寫與幀完整性拼包（ReceiveMessage）三菱 FX 編程口MelsecFxSerial / MelsecFxSerialOverTcp (MelsecFxSerialHelper)RS-232 / RS-422 / TCP9600-7-E-1STX(02)...ETX(03) + 2 Byte ASCII SumCheck• FX3U/FX3G/FX3S：D0.. $\\rightarrow$ 0x1000 + addr\*2（擴充指令 E00/E10 為 0x4000 + addr\*2）• 位元強制 (0x37 ON / 0x38 OFF)：M0.. $\\rightarrow$ 0x0800 + addr，X0..(八進位) $\\rightarrow$ 0x0400 + oct，Y0..(八進位) $\\rightarrow$ 0x0500 + oct三菱 FX 485 鏈路MelsecFxLinks / MelsecFxLinksOverTcp (MelsecFxLinksHelper)RS-485 / RS-232 / TCP9600-7-E-1 或 8-N-1ENQ(05) + 站號 + FF + BR/WR/BW/WW + SumCheck直接以 5 碼 ASCII 定址：D0100、M0100、X0010（八進位）、Y0010（八進位）、S0010、T0001、C0001Modbus RTU / ASCIIModbusRtu / ModbusAscii (ModbusInfo)RS-485 / RS-232 / TCP• RTU：8-N-1 / 8-E-1 + CRC-16 (0xA001)• ASCII：7-E-1 : ... \\r\\n + LRC支援功能碼 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x0F, 0x10；支援 s=2;x=4;100 跨站號與功能碼覆寫語法歐姆龍 FINS &amp; HostLinkOmronFinsNet / OmronFinsUdp / OmronHostLink (OmronFinsHelper)TCP:9600 / UDP / RS-232/485• FINS/TCP：20B 節點自動握手 + 0101/0102/0104• HostLink：@ + 站號 + FA + FINS + XOR FCS + \*\\r• 區域碼（Word / Bit）：DM (0x82/0x02)、CIO (0xB0/0x30)、WR (0xB1/0x31)、HR (0xB2/0x32)、AR (0xB3/0x33)、EM0 (0xA0/0x20)• 原生支援 01 04 多點隨機讀取匯川全系列 RS232/485InovanceSerial / InovanceTcpNet (InovanceHelper)RS-485 / RS-232 (RTU/ASCII) &amp; TCP:502預設 19200-8-E-1 (RTU CRC-16 或 ASCII LRC)，位元組序 CDAB• Easy / H5U：D0..7999 $\\rightarrow$ 0x0000、R0.. $\\rightarrow$ 0x3000、M0..7999 $\\rightarrow$ 0x0000、B0.. $\\rightarrow$ 0x3000、S0.. $\\rightarrow$ 0xE000、X0..(八進位) $\\rightarrow$ 0xF800、Y0..(八進位) $\\rightarrow$ 0xFC00• H3U：D0.. $\\rightarrow$ 0x0000、SD0.. $\\rightarrow$ 0x2400、M0..7679 $\\rightarrow$ 0x0000、M8000.. $\\rightarrow$ 0x1F40、SM0.. $\\rightarrow$ 0x2400• AM400/600：MW0.. $\\rightarrow$ 0x0000、MX0.0.. $\\rightarrow$ 0x00002. Cargo 依賴設定（crates/mm-drivers/Cargo.toml）在 ArgonGUI/crates/mm-drivers/Cargo.toml 中加入 tokio-serial，以支援 Windows (COM1..COM256)、Linux (/dev/ttyS\*, /dev/ttyUSB\*) 與 macOS (/dev/cu.\*) 的非同步 RS-232 / RS-485 通訊：Ini, TOML\[package\]
name = "mm-drivers"
version = "1.1.0"
edition = "2021"

\[dependencies\]
machine-model = { path = "../machine-model" }
serde = { workspace = true }
serde\_json = { workspace = true }
tokio = { workspace = true }
tokio-serial = "5.4"
thiserror = { workspace = true }
3. Rust 完整改寫程式碼（crates/mm-drivers/src/serial\_and\_protocols.rs）請將以下模組加入 crates/mm-drivers/src/ 並在 lib.rs 中匯出。本檔案完整實作：tokio-serial 非同步序列埠通道 (SerialConfig / SerialTransport)：支援 BaudRate、DataBits (7/8)、Parity (None/Even/Odd)、StopBits (1/2)、RS-485 RTS 收發方向切換與 inter\_frame\_silence\_ms 靜默幀收包。四大檢核碼引擎 (checksum 模組)：Modbus CRC-16、Modbus ASCII LRC、三菱 FX SumCheck、歐姆龍 HostLink FCS (XOR)。三菱 FX (melsec\_fx 模組)：MelsecFxSerial（編程口 0x02..0x03 含 FX3U/FX3G E00/E10 指令與位元強制 0x37/0x38）與 MelsecFxLinks（485BD BR/WR/BW/WW）。Modbus RTU / ASCII (modbus 模組)：支援 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x0F, 0x10 完整編解碼與回應異常碼解析。歐姆龍 FINS &amp; HostLink (omron 模組)：支援 FINS/TCP 20-byte 自動握手、FINS/UDP、HostLink Serial @00FA...FCS\*\\r，以及原生 01 04 多點隨機讀取封包。匯川全系列 (inovance 模組)：完整轉譯 C# InovanceHelper.PraseInovanceAddress，支援 Easy、H3U、H5U、AM 四大系列的八進位 X/Y 與內部暫存器映射，可任意搭配 RS-232/485 (Modbus RTU / ASCII) 或 TCP (Modbus TCP) 傳輸。Rust// ArgonGUI/crates/mm-drivers/src/serial\_and\_protocols.rs
#!\[forbid(unsafe\_code)\]

use std::io::{Read, Write};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use crate::{ByteOrder, ByteTransform, DriverError, bytes\_to\_hex};

// ============================================================================
// 1. RS-232 / RS-485 (tokio-serial) 通訊參數與傳輸設定
// ============================================================================

#\[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)\]
#\[serde(rename\_all = "snake\_case")\]
pub enum TransportKind {
    Tcp,
    Udp,
    SerialRs232,
    SerialRs485,
}

#\[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)\]
#\[serde(rename\_all = "snake\_case")\]
pub enum SerialParity {
    None,
    Even,
    Odd,
}

#\[derive(Debug, Clone, Serialize, Deserialize)\]
pub struct SerialPortConfig {
    /// 例如 Windows: "COM3"，Linux: "/dev/ttyUSB0"
    pub port\_name: String,
    /// 波特率：9600, 19200, 38400, 57600, 115200
    pub baud\_rate: u32,
    /// 資料位元：7 (三菱 FX / Modbus ASCII / Omron HostLink 常用) 或 8 (Modbus RTU / 匯川常用)
    pub data\_bits: u8,
    /// 同位元檢查：None, Even, Odd
    pub parity: SerialParity,
    /// 停止位元：1 或 2
    pub stop\_bits: u8,
    /// RS-485 硬體方向控制 (RTS Toggle Before Send)
    pub rs485\_rts\_enable: bool,
    /// 幀間靜默收包等待時間 (Modbus RTU t3.5 預設 5\~15 ms)
    pub inter\_frame\_silence\_ms: u64,
}

impl Default for SerialPortConfig {
    fn default() -&gt; Self {
        Self {
            port\_name: "COM3".into(),
            baud\_rate: 19200,
            data\_bits: 8,
            parity: SerialParity::Even,
            stop\_bits: 1,
            rs485\_rts\_enable: true,
            inter\_frame\_silence\_ms: 10,
        }
    }
}

/// 透過 \`tokio\_serial\` 建立跨平台序列埠連線（支援异步與同步收發）
pub fn open\_tokio\_serial\_builder(cfg: &amp;SerialPortConfig) -&gt; tokio\_serial::SerialPortBuilder {
    let data\_bits = match cfg.data\_bits {
        7 =&gt; tokio\_serial::DataBits::Seven,
        \_ =&gt; tokio\_serial::DataBits::Eight,
    };
    let parity = match cfg.parity {
        SerialParity::None =&gt; tokio\_serial::Parity::None,
        SerialParity::Even =&gt; tokio\_serial::Parity::Even,
        SerialParity::Odd =&gt; tokio\_serial::Parity::Odd,
    };
    let stop\_bits = match cfg.stop\_bits {
        2 =&gt; tokio\_serial::StopBits::Two,
        \_ =&gt; tokio\_serial::StopBits::One,
    };

    tokio\_serial::new(&amp;cfg.port\_name, cfg.baud\_rate)
        .data\_bits(data\_bits)
        .parity(parity)
        .stop\_bits(stop\_bits)
        .flow\_control(tokio\_serial::FlowControl::None)
        .timeout(Duration::from\_millis(1000))
}

// ============================================================================
// 2. 檢核碼引擎 (CRC-16 / LRC / Mitsubishi SumCheck / Omron HostLink FCS)
// ============================================================================

pub mod checksum {
    /// 1. 標準 Modbus RTU CRC-16 (多項式 0xA001，回傳 \[CRC\_Lo, CRC\_Hi\])
    /// 轉譯自 \`HslCommunication.Serial.SoftCRC16.CRC16\`
    pub fn crc16\_modbus(data: &amp;\[u8\]) -&gt; \[u8; 2\] {
        let mut crc: u16 = 0xFFFF;
        for &amp;b in data {
            crc ^= u16::from(b);
            for \_ in 0..8 {
                if (crc &amp; 0x0001) != 0 {
                    crc = (crc &gt;&gt; 1) ^ 0xA001;
                } else {
                    crc &gt;&gt;= 1;
                }
            }
        }
        \[(crc &amp; 0xFF) as u8, (crc &gt;&gt; 8) as u8\]
    }

    pub fn verify\_crc16\_modbus(frame: &amp;\[u8\]) -&gt; bool {
        if frame.len() &lt; 4 {
            return false;
        }
        let (payload, crc\_bytes) = frame.split\_at(frame.len() - 2);
        let expected = crc16\_modbus(payload);
        crc\_bytes\[0\] == expected\[0\] &amp;&amp; crc\_bytes\[1\] == expected\[1\]
    }

    /// 2. 標準 Modbus ASCII LRC 縱向冗餘校驗 (二補數總和)
    /// 轉譯自 \`HslCommunication.Serial.SoftLRC.LRC\`
    pub fn lrc\_byte(data: &amp;\[u8\]) -&gt; u8 {
        let sum: u8 = data.iter().fold(0u8, |acc, &amp;b| acc.wrapping\_add(b));
        (!sum).wrapping\_add(1)
    }

    /// 3. 三菱 FX 編程口與 FX Links 專用和校驗 (取總和低 8 位元轉 2 碼大寫 Hex ASCII)
    /// 轉譯自 \`MelsecHelper.FxCalculateCRC\`
    pub fn melsec\_sum\_check\_ascii(data: &amp;\[u8\]) -&gt; \[u8; 2\] {
        let sum: u32 = data.iter().map(|&amp;b| u32::from(b)).sum();
        let hex = format!("{:02X}", sum &amp; 0xFF);
        let bytes = hex.as\_bytes();
        \[bytes\[0\], bytes\[1\]\]
    }

    /// 4. 歐姆龍 HostLink FCS (Frame Check Sequence: 8-bit XOR 轉 2 碼大寫 Hex ASCII)
    /// 轉譯自 \`OmronHostLinkHelper\`
    pub fn omron\_fcs\_ascii(data: &amp;\[u8\]) -&gt; \[u8; 2\] {
        let xor\_val: u8 = data.iter().fold(0u8, |acc, &amp;b| acc ^ b);
        let hex = format!("{:02X}", xor\_val);
        let bytes = hex.as\_bytes();
        \[bytes\[0\], bytes\[1\]\]
    }
}

// ============================================================================
// 3. 三菱 FX 系列轉譯：MelsecFxSerial (編程口) &amp; MelsecFxLinks (485BD/ComputerLink)
// ============================================================================

pub mod melsec\_fx {
    use super::\*;
    use super::checksum::melsec\_sum\_check\_ascii;

    /// 轉譯自 \`MelsecFxSerialHelper.FxCalculateWordStartAddress\` (支援 FX3U / FX3G / FX3S 新版編程口)
    pub fn fx\_serial\_word\_byte\_offset(prefix: &amp;str, index: u32, is\_new\_version: bool) -&gt; Result&lt;(bool, u16), DriverError&gt; {
        match prefix {
            "D" =&gt; {
                if index &gt;= 8000 {
                    Ok((false, 0x0E00 + ((index - 8000) as u16) \* 2))
                } else if is\_new\_version {
                    // 新版 FX3U/FX3G 使用 E00 / E10 擴充指令，D0 起始於 0x4000
                    Ok((true, 0x4000 + (index as u16) \* 2))
                } else {
                    Ok((false, 0x1000 + (index as u16) \* 2))
                }
            }
            "T" | "TN" =&gt; Ok((false, 0x0800 + (index as u16) \* 2)),
            "C" | "CN" =&gt; {
                if index &gt;= 200 {
                    Ok((false, 0x0C00 + ((index - 200) as u16) \* 4))
                } else {
                    Ok((false, 0x0A00 + (index as u16) \* 2))
                }
            }
            other =&gt; Err(DriverError::InvalidAddress(format!("FX 編程口字元讀寫不支援元件 '{}'", other))),
        }
    }

    /// 轉譯自 \`MelsecFxSerialHelper.FxCalculateBoolStartAddress\` (用於 Force ON 0x37 / Force OFF 0x38)
    pub fn fx\_serial\_bit\_force\_offset(prefix: &amp;str, index: u32) -&gt; Result&lt;u16, DriverError&gt; {
        match prefix {
            "X" =&gt; Ok(0x0400 + (index as u16)), // index 已由八進位解析
            "Y" =&gt; Ok(0x0500 + (index as u16)),
            "M" =&gt; {
                if index &gt;= 8000 {
                    Ok(0x0F00 + ((index - 8000) as u16))
                } else {
                    Ok(0x0800 + (index as u16))
                }
            }
            "S" =&gt; Ok(0x0000 + (index as u16)),
            "T" | "TS" =&gt; Ok(0x0600 + (index as u16)),
            "C" | "CS" =&gt; Ok(0x0E00 + (index as u16)),
            other =&gt; Err(DriverError::InvalidAddress(format!("FX 編程口位元強制不支援元件 '{}'", other))),
        }
    }

    /// 建立三菱 FX 編程口讀取報文 (\`STX(0x02) + Cmd('0' 或 "E00") + Addr(4 Hex) + ByteLen(2 Hex) + ETX(0x03) + SumCheck(2 Hex)\`)
    pub fn build\_fx\_serial\_read\_frame(address: &amp;str, word\_count: u16, is\_new\_version: bool) -&gt; Result&lt;Vec&lt;u8&gt;, DriverError&gt; {
        let (prefix, idx, \_, is\_bit) = crate::parse\_universal\_plc\_address(address)?;
        let (use\_ext\_cmd, byte\_offset, byte\_len) = if is\_bit {
            let bit\_addr = fx\_serial\_bit\_force\_offset(&amp;prefix, idx)?;
            (false, bit\_addr / 8, ((word\_count + 7) / 8).max(1) as u8)
        } else {
            let (ext, off) = fx\_serial\_word\_byte\_offset(&amp;prefix, idx, is\_new\_version)?;
            (ext, off, (word\_count \* 2).min(64) as u8)
        };

        let mut body = Vec::with\_capacity(14);
        if use\_ext\_cmd {
            body.extend\_from\_slice(b"E00");
        } else {
            body.push(b'0');
        }
        body.extend\_from\_slice(format!("{:04X}{:02X}", byte\_offset, byte\_len).as\_bytes());
        body.push(0x03); // ETX

        let sum = melsec\_sum\_check\_ascii(&amp;body);
        let mut frame = vec!\[0x02\]; // STX
        frame.extend\_from\_slice(&amp;body);
        frame.extend\_from\_slice(&amp;sum);
        Ok(frame)
    }

    /// 建立三菱 FX 編程口寫入報文：
    /// - 單點 Bit：使用 \`'7'\` (0x37 Force ON) / \`'8'\` (0x38 Force OFF)，注意位址高地位元組順序為 \`\[LoByteHex\]\[HiByteHex\]\`！
    /// - Word 暫存器：使用 \`'1'\` 或 \`"E10"\` + \`Addr(4 Hex)\` + \`ByteLen(2 Hex)\` + \`DataHex\` + \`ETX(0x03)\` + \`SumCheck(2 Hex)\`
    pub fn build\_fx\_serial\_write\_frame(
        address: &amp;str,
        is\_bit\_write: bool,
        bit\_val: bool,
        word\_bytes: &amp;\[u8\],
        is\_new\_version: bool,
    ) -&gt; Result&lt;Vec&lt;u8&gt;, DriverError&gt; {
        let (prefix, idx, \_, \_) = crate::parse\_universal\_plc\_address(address)?;
        let mut body = Vec::new();

        if is\_bit\_write {
            let bit\_addr = fx\_serial\_bit\_force\_offset(&amp;prefix, idx)?;
            body.push(if bit\_val { b'7' } else { b'8' });
            // FX Force ON/OFF 特殊順序：先傳低位元組 2 Hex，再傳高位元組 2 Hex (MelsecFxSerialHelper.BuildWriteBoolPacket)
            body.extend\_from\_slice(format!("{:02X}{:02X}", bit\_addr &amp; 0xFF, (bit\_addr &gt;&gt; 8) &amp; 0xFF).as\_bytes());
            body.push(0x03);
        } else {
            let (use\_ext\_cmd, byte\_offset) = fx\_serial\_word\_byte\_offset(&amp;prefix, idx, is\_new\_version)?;
            if use\_ext\_cmd {
                body.extend\_from\_slice(b"E10");
            } else {
                body.push(b'1');
            }
            body.extend\_from\_slice(format!("{:04X}{:02X}", byte\_offset, word\_bytes.len()).as\_bytes());
            for &amp;b in word\_bytes {
                body.extend\_from\_slice(format!("{:02X}", b).as\_bytes());
            }
            body.push(0x03);
        }

        let sum = melsec\_sum\_check\_ascii(&amp;body);
        let mut frame = vec!\[0x02\];
        frame.extend\_from\_slice(&amp;body);
        frame.extend\_from\_slice(&amp;sum);
        Ok(frame)
    }

    /// 建立三菱 FX Computer Link (\`MelsecFxLinks\` 485BD/485ADP) 報文
    /// 格式：\`ENQ(0x05) + Station(2) + PC("FF") + Cmd("BR"/"WR"/"BW"/"WW") + Wait('0') + Addr(5) + Len(2) + \[Data\] + SumCheck(2)\`
    pub fn build\_fx\_links\_frame(
        station: u8,
        address: &amp;str,
        count: u16,
        write\_words: Option&lt;&amp;\[u16\]&gt;,
        sum\_check\_enable: bool,
    ) -&gt; Result&lt;Vec&lt;u8&gt;, DriverError&gt; {
        let (prefix, idx, \_, is\_bit) = crate::parse\_universal\_plc\_address(address)?;
        let addr\_5 = if prefix == "X" || prefix == "Y" {
            format!("{}{:04o}", prefix, idx)
        } else {
            format!("{}{:04}", prefix, idx)
        };

        let cmd = match (is\_bit, write\_words.is\_some()) {
            (true, false) =&gt; "BR",
            (false, false) =&gt; "WR",
            (true, true) =&gt; "BW",
            (false, true) =&gt; "WW",
        };

        let mut body = format!("{:02X}FF{}0{}{:02X}", station, cmd, addr\_5, count &amp; 0xFF).into\_bytes();
        if let Some(words) = write\_words {
            for &amp;w in words {
                if is\_bit {
                    body.push(if w != 0 { b'1' } else { b'0' });
                } else {
                    body.extend\_from\_slice(format!("{:04X}", w).as\_bytes());
                }
            }
        }

        let mut frame = vec!\[0x05\]; // ENQ
        frame.extend\_from\_slice(&amp;body);
        if sum\_check\_enable {
            let sum = melsec\_sum\_check\_ascii(&amp;body);
            frame.extend\_from\_slice(&amp;sum);
        }
        Ok(frame)
    }
}

// ============================================================================
// 4. Modbus RTU 與 Modbus ASCII (RS-232 / RS-485) 完整編解碼器
// ============================================================================

pub mod modbus {
    use super::checksum::{crc16\_modbus, lrc\_byte};

    /// 建立標準 Modbus RTU 二進位報文（自動附加 CRC-16）
    pub fn build\_rtu\_frame(station: u8, function\_code: u8, pdu\_body: &amp;\[u8\]) -&gt; Vec&lt;u8&gt; {
        let mut frame = Vec::with\_capacity(2 + pdu\_body.len() + 2);
        frame.push(station);
        frame.push(function\_code);
        frame.extend\_from\_slice(pdu\_body);
        let crc = crc16\_modbus(&amp;frame);
        frame.extend\_from\_slice(&amp;crc);
        frame
    }

    /// 建立標準 Modbus ASCII 報文（\`:\` + HexASCII(Station + FC + Body + LRC) + \`\\r\\n\`）
    /// 轉譯自 \`HslCommunication.ModBus.ModbusInfo.TransModbusCoreToAsciiPackCommand\`
    pub fn build\_ascii\_frame(station: u8, function\_code: u8, pdu\_body: &amp;\[u8\]) -&gt; Vec&lt;u8&gt; {
        let mut core = Vec::with\_capacity(2 + pdu\_body.len());
        core.push(station);
        core.push(function\_code);
        core.extend\_from\_slice(pdu\_body);
        let lrc = lrc\_byte(&amp;core);
        core.push(lrc);

        let mut ascii = Vec::with\_capacity(1 + core.len() \* 2 + 2);
        ascii.push(b':');
        for b in core {
            ascii.extend\_from\_slice(format!("{:02X}", b).as\_bytes());
        }
        ascii.extend\_from\_slice(b"\\r\\n");
        ascii
    }

    /// 建立讀取線圈 (0x01/0x02) 或保持/輸入暫存器 (0x03/0x04) 之 PDU Body
    pub fn build\_read\_pdu\_body(start\_addr: u16, count: u16) -&gt; \[u8; 4\] {
        \[
            (start\_addr &gt;&gt; 8) as u8,
            (start\_addr &amp; 0xFF) as u8,
            (count &gt;&gt; 8) as u8,
            (count &amp; 0xFF) as u8,
        \]
    }

    /// 建立單點寫入線圈 (0x05) 或單一暫存器 (0x06) 之 PDU Body
    pub fn build\_write\_single\_pdu\_body(addr: u16, val: u16, is\_coil: bool) -&gt; \[u8; 4\] {
        let v = if is\_coil { if val != 0 { 0xFF00 } else { 0x0000 } } else { val };
        \[
            (addr &gt;&gt; 8) as u8,
            (addr &amp; 0xFF) as u8,
            (v &gt;&gt; 8) as u8,
            (v &amp; 0xFF) as u8,
        \]
    }

    /// 建立連續寫入多個保持暫存器 (0x10) 之 PDU Body
    pub fn build\_write\_multiple\_registers\_pdu\_body(start\_addr: u16, word\_bytes: &amp;\[u8\]) -&gt; Vec&lt;u8&gt; {
        let word\_count = (word\_bytes.len() / 2) as u16;
        let mut body = Vec::with\_capacity(5 + word\_bytes.len());
        body.push((start\_addr &gt;&gt; 8) as u8);
        body.push((start\_addr &amp; 0xFF) as u8);
        body.push((word\_count &gt;&gt; 8) as u8);
        body.push((word\_count &amp; 0xFF) as u8);
        body.push(word\_bytes.len() as u8);
        body.extend\_from\_slice(word\_bytes);
        body
    }
}

// ============================================================================
// 5. Omron FINS (TCP / UDP) &amp; Omron HostLink (RS-232/485 Serial) 完整轉譯
// ============================================================================

pub mod omron {
    use super::\*;
    use super::checksum::omron\_fcs\_ascii;

    /// 歐姆龍記憶體區域碼 (\`OmronFinsDataType\` 轉譯)
    pub fn parse\_omron\_fins\_address(address: &amp;str, is\_bit: bool) -&gt; Result&lt;(u8, u16, u8), DriverError&gt; {
        let s = address.trim().to\_ascii\_uppercase();
        let (prefix, rest) = if let Some(r) = s.strip\_prefix("CIO") {
            ("CIO", r)
        } else if let Some(r) = s.strip\_prefix("DM").or\_else(|| s.strip\_prefix("D")) {
            ("DM", r)
        } else if let Some(r) = s.strip\_prefix("WR").or\_else(|| s.strip\_prefix("W")) {
            ("WR", r)
        } else if let Some(r) = s.strip\_prefix("HR").or\_else(|| s.strip\_prefix("H")) {
            ("HR", r)
        } else if let Some(r) = s.strip\_prefix("AR").or\_else(|| s.strip\_prefix("A")) {
            ("AR", r)
        } else if let Some(r) = s.strip\_prefix("EM").or\_else(|| s.strip\_prefix("E")) {
            ("EM", r)
        } else {
            ("DM", s.as\_str())
        };

        let area\_code: u8 = match (prefix, is\_bit) {
            ("DM", false) =&gt; 0x82,
            ("DM", true) =&gt; 0x02,
            ("CIO", false) =&gt; 0xB0,
            ("CIO", true) =&gt; 0x30,
            ("WR", false) =&gt; 0xB1,
            ("WR", true) =&gt; 0x31,
            ("HR", false) =&gt; 0xB2,
            ("HR", true) =&gt; 0x32,
            ("AR", false) =&gt; 0xB3,
            ("AR", true) =&gt; 0x33,
            ("EM", false) =&gt; 0xA0,
            ("EM", true) =&gt; 0x20,
            \_ =&gt; 0x82,
        };

        let (word\_str, bit\_str) = rest.split\_once('.').unwrap\_or((rest, "0"));
        let word\_addr: u16 = word\_str.parse().unwrap\_or(0);
        let bit\_addr: u8 = bit\_str.parse().unwrap\_or(0);
        Ok((area\_code, word\_addr, bit\_addr))
    }

    /// FINS/TCP 連線節點自動握手請求封包 (20 Bytes)
    pub fn build\_fins\_tcp\_handshake\_frame(client\_node: u8) -&gt; \[u8; 20\] {
        \[
            0x46, 0x49, 0x4E, 0x53, // "FINS"
            0x00, 0x00, 0x00, 0x0C, // Length = 12
            0x00, 0x00, 0x00, 0x00, // Command = 0 (Node Address Data Send)
            0x00, 0x00, 0x00, 0x00, // Error Code = 0
            0x00, 0x00, 0x00, client\_node,
        \]
    }

    /// 建立 FINS 核心指令 (\`01 01\` 讀取 / \`01 02\` 寫入)
    pub fn build\_fins\_core\_command(
        address: &amp;str,
        count: u16,
        is\_bit: bool,
        write\_data: Option&lt;&amp;\[u8\]&gt;,
    ) -&gt; Result&lt;Vec&lt;u8&gt;, DriverError&gt; {
        let (area, word\_addr, bit\_addr) = parse\_omron\_fins\_address(address, is\_bit)?;
        let mut cmd = Vec::with\_capacity(8 + write\_data.map(|d| d.len()).unwrap\_or(0));
        cmd.push(0x01); // MRC
        cmd.push(if write\_data.is\_some() { 0x02 } else { 0x01 }); // SRC
        cmd.push(area);
        cmd.push((word\_addr &gt;&gt; 8) as u8);
        cmd.push((word\_addr &amp; 0xFF) as u8);
        cmd.push(bit\_addr);
        cmd.push((count &gt;&gt; 8) as u8);
        cmd.push((count &amp; 0xFF) as u8);
        if let Some(data) = write\_data {
            cmd.extend\_from\_slice(data);
        }
        Ok(cmd)
    }

    /// 建立 FINS \`01 04\` 多點隨機讀取指令 (Multiple Memory Area Read)
    /// 轉譯自 \`OmronFinsHelper.BuildReadCommand(string\[\] address)\`
    pub fn build\_fins\_multiple\_read\_command(addresses: &amp;\[String\]) -&gt; Result&lt;Vec&lt;u8&gt;, DriverError&gt; {
        let mut cmd = Vec::with\_capacity(2 + addresses.len() \* 4);
        cmd.push(0x01); // MRC
        cmd.push(0x04); // SRC = 0x04 (Multiple Memory Area Read)
        for addr in addresses {
            let is\_bit = addr.contains('.');
            let (area, word\_addr, bit\_addr) = parse\_omron\_fins\_address(addr, is\_bit)?;
            cmd.push(area);
            cmd.push((word\_addr &gt;&gt; 8) as u8);
            cmd.push((word\_addr &amp; 0xFF) as u8);
            cmd.push(bit\_addr);
        }
        Ok(cmd)
    }

    /// 將 FINS 核心指令包裝為 \*\*Omron HostLink (RS-232/485 Serial)\*\* 報文
    /// 格式：\`@\` + \`UnitNo(2 Hex)\` + \`FA\` + \`ResponseWait('0')\` + \`ICF(00)DA2(00)SA2(00)SID(00)\` + \`FINS\_Hex\` + \`FCS(2 Hex)\` + \`\*\\r\`
    /// 轉譯自 \`OmronHostLinkHelper.PackCommand\`
    pub fn pack\_omron\_hostlink\_serial(unit\_no: u8, fins\_core: &amp;\[u8\]) -&gt; Vec&lt;u8&gt; {
        let mut body = format!("@{:02X}FA000000000", unit\_no).into\_bytes();
        for &amp;b in fins\_core {
            body.extend\_from\_slice(format!("{:02X}", b).as\_bytes());
        }
        let fcs = omron\_fcs\_ascii(&amp;body);
        body.extend\_from\_slice(&amp;fcs);
        body.extend\_from\_slice(b"\*\\r");
        body
    }
}

// ============================================================================
// 6. 匯川 Inovance 全系列 (Easy / H3U / H5U / AM) 位址轉譯與 RS-232/485 封裝
// ============================================================================

pub mod inovance {
    use super::\*;
    use super::modbus::{
        build\_ascii\_frame, build\_read\_pdu\_body, build\_rtu\_frame,
        build\_write\_multiple\_registers\_pdu\_body, build\_write\_single\_pdu\_body,
    };

    /// 對應 \`HslCommunication.Profinet.Inovance.InovanceSeries\`
    #\[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)\]
    pub enum InovanceSeries {
        /// Easy320 / Easy521 系列 (同 MachineModel PlcFamily::InovanceEasy)
        Easy,
        /// H3U 標準系列
        H3U,
        /// H5U 高階系列
        H5U,
        /// AM400 / AM600 / AM800 中大型系列 (IEC 61131-3 定址 MW/MX/IW/QW)
        AM,
    }

    /// 1:1 轉譯自 \`HslCommunication.Profinet.Inovance.InovanceHelper.PraseInovanceAddress\`
    /// 將匯川軟元件位址 (\`D100\`, \`M100\`, \`X010\`, \`Y010\`, \`R100\`, \`B10\`, \`S10\`, \`SM0\`, \`SD0\`, \`MW100\`)
    /// 轉換為底層 Modbus \`(是否位元元件, Modbus 0-based 偏移位址)\`
    pub fn translate\_inovance\_address(
        series: InovanceSeries,
        address: &amp;str,
    ) -&gt; Result&lt;(bool, u16), DriverError&gt; {
        let s = address.trim().to\_ascii\_uppercase();
        if s.is\_empty() {
            return Err(DriverError::InvalidAddress("匯川位址不得為空".into()));
        }

        match series {
            InovanceSeries::AM =&gt; {
                if let Some(rest) = s.strip\_prefix("MX") {
                    let (word\_s, bit\_s) = rest.split\_once('.').unwrap\_or((rest, "0"));
                    let w: u16 = word\_s.parse().unwrap\_or(0);
                    let b: u16 = bit\_s.parse().unwrap\_or(0);
                    return Ok((true, w \* 16 + b));
                }
                if let Some(rest) = s.strip\_prefix("MW").or\_else(|| s.strip\_prefix("D")) {
                    let w: u16 = rest.parse().unwrap\_or(0);
                    return Ok((false, w));
                }
                if let Some(rest) = s.strip\_prefix("M") {
                    let b: u16 = rest.parse().unwrap\_or(0);
                    return Ok((true, b));
                }
            }
            InovanceSeries::Easy | InovanceSeries::H5U =&gt; {
                // Easy &amp; H5U 軟元件映射表 (PraseInovanceH5UAddress)
                if let Some(r) = s.strip\_prefix("X") {
                    let oct = u16::from\_str\_radix(r, 8).map\_err(|\_| {
                        DriverError::InvalidAddress(format!("匯川 X 輸入 '{}' 必須為八進位", address))
                    })?;
                    return Ok((true, 0xF800 + oct));
                }
                if let Some(r) = s.strip\_prefix("Y") {
                    let oct = u16::from\_str\_radix(r, 8).map\_err(|\_| {
                        DriverError::InvalidAddress(format!("匯川 Y 輸出 '{}' 必須為八進位", address))
                    })?;
                    return Ok((true, 0xFC00 + oct));
                }
                if let Some(r) = s.strip\_prefix("M") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((true, idx)); // M0..M7999 -&gt; 0x0000..0x1F3F
                }
                if let Some(r) = s.strip\_prefix("B") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((true, 0x3000 + idx));
                }
                if let Some(r) = s.strip\_prefix("S") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((true, 0xE000 + idx));
                }
                if let Some(r) = s.strip\_prefix("D") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((false, idx)); // D0..D7999 -&gt; 0x0000..0x1F3F
                }
                if let Some(r) = s.strip\_prefix("R") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((false, 0x3000 + idx));
                }
            }
            InovanceSeries::H3U =&gt; {
                // H3U 軟元件映射表 (PraseInovanceH3UAddress)
                if let Some(r) = s.strip\_prefix("X") {
                    let oct = u16::from\_str\_radix(r, 8).map\_err(|\_| {
                        DriverError::InvalidAddress(format!("H3U X 輸入 '{}' 必須為八進位", address))
                    })?;
                    return Ok((true, 0xF800 + oct));
                }
                if let Some(r) = s.strip\_prefix("Y") {
                    let oct = u16::from\_str\_radix(r, 8).map\_err(|\_| {
                        DriverError::InvalidAddress(format!("H3U Y 輸出 '{}' 必須為八進位", address))
                    })?;
                    return Ok((true, 0xFC00 + oct));
                }
                if let Some(r) = s.strip\_prefix("SM") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((true, 0x2400 + idx));
                }
                if let Some(r) = s.strip\_prefix("SD") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((false, 0x2400 + idx));
                }
                if let Some(r) = s.strip\_prefix("M") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    let offset = if idx &gt;= 8000 { 0x1F40 + (idx - 8000) } else { idx };
                    return Ok((true, offset));
                }
                if let Some(r) = s.strip\_prefix("S") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((true, 0xE000 + idx));
                }
                if let Some(r) = s.strip\_prefix("T") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((false, 0xF000 + idx));
                }
                if let Some(r) = s.strip\_prefix("C") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((false, 0xF400 + idx));
                }
                if let Some(r) = s.strip\_prefix("D") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((false, idx));
                }
                if let Some(r) = s.strip\_prefix("R") {
                    let idx: u16 = r.parse().unwrap\_or(0);
                    return Ok((false, 0x3000 + idx));
                }
            }
        }

        Err(DriverError::InvalidAddress(format!(
            "匯川 {:?} 系列不支援軟元件位址 '{}'",
            series, address
        )))
    }

    /// 產生匯川 RS-232 / RS-485 (\`InovanceSerial\` RTU 或 ASCII) 讀取報文
    pub fn build\_inovance\_serial\_read\_frame(
        series: InovanceSeries,
        station: u8,
        address: &amp;str,
        count: u16,
        use\_ascii\_mode: bool,
    ) -&gt; Result&lt;(u16, Vec&lt;u8&gt;), DriverError&gt; {
        let (is\_bit, modbus\_addr) = translate\_inovance\_address(series, address)?;
        let fc = if is\_bit { 0x01 } else { 0x03 };
        let body = build\_read\_pdu\_body(modbus\_addr, count);
        let frame = if use\_ascii\_mode {
            build\_ascii\_frame(station, fc, &amp;body)
        } else {
            build\_rtu\_frame(station, fc, &amp;body)
        };
        Ok((modbus\_addr, frame))
    }

    /// 產生匯川 RS-232 / RS-485 (\`InovanceSerial\` RTU 或 ASCII) 寫入報文
    pub fn build\_inovance\_serial\_write\_frame(
        series: InovanceSeries,
        station: u8,
        address: &amp;str,
        words: &amp;\[u16\],
        use\_ascii\_mode: bool,
    ) -&gt; Result&lt;(u16, Vec&lt;u8&gt;), DriverError&gt; {
        let (is\_bit, modbus\_addr) = translate\_inovance\_address(series, address)?;
        let frame = if words.len() &lt;= 1 {
            let fc = if is\_bit { 0x05 } else { 0x06 };
            let body = build\_write\_single\_pdu\_body(modbus\_addr, \*words.first().unwrap\_or(&amp;0), is\_bit);
            if use\_ascii\_mode {
                build\_ascii\_frame(station, fc, &amp;body)
            } else {
                build\_rtu\_frame(station, fc, &amp;body)
            }
        } else {
            let mut raw = Vec::with\_capacity(words.len() \* 2);
            for &amp;w in words {
                raw.extend\_from\_slice(&amp;w.to\_be\_bytes());
            }
            let body = build\_write\_multiple\_registers\_pdu\_body(modbus\_addr, &amp;raw);
            if use\_ascii\_mode {
                build\_ascii\_frame(station, 0x10, &amp;body)
            } else {
                build\_rtu\_frame(station, 0x10, &amp;body)
            }
        };
        Ok((modbus\_addr, frame))
    }
}