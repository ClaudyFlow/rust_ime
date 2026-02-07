#[cfg(windows)]
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::System::Com::*,
    Win32::System::SystemServices::DLL_PROCESS_ATTACH,
};

#[cfg(windows)]
mod registry;
#[cfg(windows)]
mod text_service;
#[cfg(windows)]
mod class_factory;

#[cfg(windows)]
use crate::class_factory::ClassFactory;

#[cfg(windows)]
// 这是一个随机生成的 GUID，正式发布时请保持固定
// {C03C9525-2C5E-4959-9988-51787281D523}
pub const IME_ID: GUID = GUID::from_u128(0xc03c9525_2c5e_4959_9988_51787281d523);

#[cfg(windows)]
// 语言配置文件 GUID (简体中文)
pub const LANG_PROFILE_ID: GUID = GUID::from_u128(0xc03c9525_2c5e_4959_9988_51787281d524);

#[cfg(windows)]
static mut DLL_INSTANCE: HINSTANCE = HINSTANCE(0);

#[cfg(windows)]
#[no_mangle]
#[allow(non_snake_case)]
unsafe extern "system" fn DllMain(
    dll_module: HINSTANCE,
    call_reason: u32,
    _reserved: *mut std::ffi::c_void,
) -> bool {
    match call_reason {
        DLL_PROCESS_ATTACH => {
            DLL_INSTANCE = dll_module;
        }
        _ => {}
    }
    true
}

#[cfg(windows)]
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut std::ffi::c_void,
) -> HRESULT {
    // 检查请求的 CLSID 是否是我们的 IME_ID
    if *rclsid != IME_ID {
        return CLASS_E_CLASSNOTAVAILABLE;
    }

    // 创建类工厂
    let factory = ClassFactory::new();
    let unknown: IUnknown = factory.into();
    
    // 查询接口 (通常是 IClassFactory)
    unknown.query(&*riid, ppv)
}

#[cfg(windows)]
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllRegisterServer() -> HRESULT {
    registry::register_server(DLL_INSTANCE, &IME_ID, "Rust IME")
        .map_or_else(|e| e.code(), |_| S_OK)
}

#[cfg(windows)]
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllUnregisterServer() -> HRESULT {
    registry::unregister_server(&IME_ID)
        .map_or_else(|e| e.code(), |_| S_OK)
}

// 空壳实现，防止编译错误
#[cfg(not(windows))]
#[no_mangle]
pub extern "C" fn placeholder() {}
