// ArgonGUI/src-tauri/src/driver_commands.rs
use std::sync::Mutex;
use plc_common::ScalarValue;
use mm_drivers::{
    ContinuousReadResult, DriverCatalogItem, DriverConfig, DriverTrace,
    LatencyReport, PlcDriver, RandomReadItem,
    RandomReadResult, RandomWriteItem, SingleReadResult, StressTestConfig,
    StressTestReport, UniversalPlcDriver, WriteAck, list_driver_catalog,
};

pub struct DriverManagerState {
    pub active_driver: Mutex<UniversalPlcDriver>,
}

impl Default for DriverManagerState {
    fn default() -> Self {
        let mut drv = UniversalPlcDriver::new(DriverConfig::default());
        let _ = drv.connect(DriverConfig::default());
        Self { active_driver: Mutex::new(drv) }
    }
}

#[tauri::command]
pub fn driver_list_catalog() -> Vec<DriverCatalogItem> {
    list_driver_catalog()
}

#[tauri::command]
pub fn driver_connect(
    state: tauri::State<'_, DriverManagerState>,
    config: DriverConfig,
) -> Result<DriverTrace, String> {
    let mut drv = state.active_driver.lock().map_err(|e| e.to_string())?;
    drv.connect(config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn driver_disconnect(state: tauri::State<'_, DriverManagerState>) -> Result<(), String> {
    let mut drv = state.active_driver.lock().map_err(|e| e.to_string())?;
    drv.disconnect().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn driver_read_single(
    state: tauri::State<'_, DriverManagerState>,
    address: String,
    data_type: String,
) -> Result<SingleReadResult, String> {
    let mut drv = state.active_driver.lock().map_err(|e| e.to_string())?;
    drv.read_single(&address, &data_type).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn driver_write_single(
    state: tauri::State<'_, DriverManagerState>,
    address: String,
    data_type: String,
    value: ScalarValue,
) -> Result<WriteAck, String> {
    let mut drv = state.active_driver.lock().map_err(|e| e.to_string())?;
    drv.write_single(&address, &data_type, value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn driver_read_continuous(
    state: tauri::State<'_, DriverManagerState>,
    start_address: String,
    length: u16,
    data_type: String,
) -> Result<ContinuousReadResult, String> {
    let mut drv = state.active_driver.lock().map_err(|e| e.to_string())?;
    drv.read_continuous(&start_address, length, &data_type).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn driver_write_continuous(
    state: tauri::State<'_, DriverManagerState>,
    start_address: String,
    data_type: String,
    values: Vec<ScalarValue>,
) -> Result<WriteAck, String> {
    let mut drv = state.active_driver.lock().map_err(|e| e.to_string())?;
    drv.write_continuous(&start_address, &data_type, &values).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn driver_read_random(
    state: tauri::State<'_, DriverManagerState>,
    items: Vec<RandomReadItem>,
) -> Result<RandomReadResult, String> {
    let mut drv = state.active_driver.lock().map_err(|e| e.to_string())?;
    drv.read_random(&items).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn driver_write_random(
    state: tauri::State<'_, DriverManagerState>,
    items: Vec<RandomWriteItem>,
) -> Result<Vec<WriteAck>, String> {
    let mut drv = state.active_driver.lock().map_err(|e| e.to_string())?;
    drv.write_random(&items).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn driver_ping_latency(
    state: tauri::State<'_, DriverManagerState>,
    address: String,
    rounds: u32,
) -> Result<LatencyReport, String> {
    let mut drv = state.active_driver.lock().map_err(|e| e.to_string())?;
    drv.ping_latency(&address, rounds).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn driver_stress_test(
    state: tauri::State<'_, DriverManagerState>,
    config: StressTestConfig,
) -> Result<StressTestReport, String> {
    let mut drv = state.active_driver.lock().map_err(|e| e.to_string())?;
    drv.run_stress_test(&config).map_err(|e| e.to_string())
}
