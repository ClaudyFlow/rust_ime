use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::System::Com::*,
};
use std::sync::atomic::{AtomicU32, Ordering};
use crate::text_service::TextService;

#[implement(IClassFactory)]
pub struct ClassFactory {
    ref_count: AtomicU32,
}

impl ClassFactory {
    pub fn new() -> Self {
        Self {
            ref_count: AtomicU32::new(1),
        }
    }
}

impl IClassFactory_Impl for ClassFactory {
    fn CreateInstance(
        &self,
        p_unk_outer: Option<&IUnknown>,
        riid: *const GUID,
        ppv_object: *mut *mut std::ffi::c_void,
    ) -> Result<()> {
        // ClassFactory 不支持聚合
        if p_unk_outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION);
        }

        // 创建我们的 TextService 实例
        let service = TextService::new();
        // 将 Rust 结构体转换为 COM 接口 (IUnknown)
        let unknown: IUnknown = service.into();
        
        // 查询请求的接口 (riid)
        unsafe { unknown.query(&*riid, ppv_object).ok() }
    }

    fn LockServer(&self, _f_lock: BOOL) -> Result<()> {
        // 可以用来锁定 DLL 不被卸载，这里暂时留空
        Ok(())
    }
}
