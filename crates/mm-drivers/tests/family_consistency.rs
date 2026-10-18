//! 家族表（`plc-common`）與 `DriverKind`／驅動型錄的一致性檢查。

use plc_common::Family as PlcFamily;
use mm_drivers::{build_protocol_frame, list_driver_catalog, modbus_crc16, ByteOrder, DriverKind};
use serde_json::Value;

fn plc_catalog_names() -> Vec<String> {
    list_driver_catalog()
        .into_iter()
        .filter(|c| c.category == "PLC")
        .map(|c| match serde_json::to_value(c.kind).unwrap() {
            Value::String(s) => s,
            other => panic!("DriverKind 序列化結果不是字串：{other:?}"),
        })
        .collect()
}

#[test]
fn every_family_has_a_driver_kind_and_plc_catalog_entry() {
    let names = plc_catalog_names();
    for f in PlcFamily::ALL {
        let kind = serde_json::from_value::<DriverKind>(Value::String(f.as_str().into()))
            .unwrap_or_else(|e| panic!("家族 {} 沒有對應的 DriverKind：{e}", f.as_str()));
        assert!(
            names.iter().any(|n| n == f.as_str()),
            "家族 {} ({kind:?}) 不在 PLC 型錄中",
            f.as_str()
        );
    }
}

#[test]
fn every_plc_catalog_entry_is_a_family() {
    for n in plc_catalog_names() {
        serde_json::from_value::<PlcFamily>(Value::String(n.clone()))
            .unwrap_or_else(|e| panic!("PLC 型錄項目 {n} 不是 PlcFamily：{e}"));
    }
}

#[test]
fn plc_counts_match() {
    assert_eq!(PlcFamily::ALL.len(), plc_catalog_names().len());
    assert_eq!(PlcFamily::ALL.len(), 56);
}

#[test]
fn inovance_easy_serial_frame_is_rtu() {
    let frame = build_protocol_frame(DriverKind::InovanceEasySerial, 1, "D100", 1, false);
    let mut expected = vec![0x01, 0x03, 0x00, 0x64, 0x00, 0x01];
    expected.extend_from_slice(&modbus_crc16(&expected));
    assert_eq!(frame, expected);
}

#[test]
fn inovance_easy_serial_defaults() {
    assert_eq!(DriverKind::InovanceEasySerial.default_byte_order(), ByteOrder::CDAB);
    assert_eq!(DriverKind::InovanceEasySerial.default_port(), 0);
}
