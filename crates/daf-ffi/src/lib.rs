#![allow(clippy::not_unsafe_ptr_arg_deref, clippy::assertions_on_constants)]

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::panic;
use std::sync::{Arc, Mutex, OnceLock};

use daf_algorithms::FibonacciDP;
use daf_application::DataAccess;
use daf_cache::MemoryCache;
use daf_core::{
    DataAccessError, DeleteInfo, JsonValue, PostInfo, PutInfo, QueryInfo, ResourceId, UserId,
    ValidationError,
};
use daf_repository::MemoryRepository;

thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<CString>> = const { std::cell::RefCell::new(None) };
}

static LIVE_HANDLES: OnceLock<Mutex<HashMap<usize, u64>>> = OnceLock::new();

fn live_handles() -> &'static Mutex<HashMap<usize, u64>> {
    let handles = LIVE_HANDLES.get_or_init(|| Mutex::new(HashMap::new()));
    debug_assert!(
        !handles.is_poisoned(),
        "LIVE_HANDLES mutex must not be poisoned"
    );
    handles
}

fn set_last_error(msg: &str) {
    debug_assert!(!msg.is_empty(), "error message must not be empty");
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = Some(CString::new(msg).unwrap());
    });
}

fn clear_last_error() {
    debug_assert!(
        LAST_ERROR.with(|s| s.borrow().is_none()),
        "LAST_ERROR already set before clear"
    );
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

#[repr(C)]
#[derive(PartialEq)]
pub enum DafErrorCode {
    Ok = 0,
    InvalidArgument = 1,
    NotFound = 2,
    AuthorizationFailed = 3,
    ValidationFailed = 4,
    InternalError = 5,
}

fn map_data_access_error(err: DataAccessError) -> DafErrorCode {
    let code = match err {
        DataAccessError::NotFound(_) => DafErrorCode::NotFound,
        DataAccessError::Authorization(_) => DafErrorCode::AuthorizationFailed,
        DataAccessError::Validation(_) => DafErrorCode::ValidationFailed,
        _ => DafErrorCode::InternalError,
    };
    debug_assert!(
        code != DafErrorCode::InternalError
            || !matches!(
                err,
                DataAccessError::NotFound(_)
                    | DataAccessError::Authorization(_)
                    | DataAccessError::Validation(_)
            ),
        "known error variant must not map to InternalError"
    );
    code
}

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    debug_assert!(RUNTIME.get().is_some(), "tokio runtime not initialised");
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("failed to create tokio runtime"))
}

fn block_on<F>(f: F) -> F::Output
where
    F: std::future::Future,
{
    debug_assert!(RUNTIME.get().is_some(), "tokio runtime not initialised");
    runtime().block_on(f)
}

fn validate_utf8_cstr(ptr: *const c_char, label: &str) -> Result<&'static str, DataAccessError> {
    debug_assert!(
        !ptr.is_null(),
        "null ptr must be caught before validate_utf8_cstr"
    );
    if ptr.is_null() {
        set_last_error(&format!("null pointer: {label}"));
        return Err(ValidationError::new(format!("null pointer: {label}")).into());
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().map_err(|_| {
        set_last_error(&format!("invalid utf-8 in {label}"));
        ValidationError::new(format!("invalid utf-8 in {label}")).into()
    })
}

#[no_mangle]
pub extern "C" fn daf_data_access_new() -> *mut DataAccess {
    clear_last_error();
    match panic::catch_unwind(|| {
        let repo = Arc::new(MemoryRepository::<JsonValue>::new());
        let cache = Arc::new(MemoryCache::new(1024));
        let mut algorithms = HashMap::new();
        algorithms.insert(
            "fib".to_string(),
            Arc::new(FibonacciDP::new()) as Arc<dyn daf_core::Algorithm>,
        );
        let daf = DataAccess::new(repo, cache, Some(algorithms), None);
        let raw = Box::into_raw(Box::new(daf));
        live_handles().lock().unwrap().insert(raw as usize, 0);
        raw
    }) {
        Ok(ptr) => ptr,
        Err(_) => {
            set_last_error("panic in daf_data_access_new");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn daf_data_access_free(ptr: *mut DataAccess) -> c_int {
    if ptr.is_null() {
        return DafErrorCode::InvalidArgument as c_int;
    }
    let handle = ptr as usize;
    let mut handles = live_handles().lock().unwrap();
    if !handles.contains_key(&handle) {
        return DafErrorCode::InvalidArgument as c_int;
    }
    handles.remove(&handle);
    drop(handles);
    unsafe { drop(Box::from_raw(ptr)) };
    DafErrorCode::Ok as c_int
}

fn catch_panic<F, T>(f: F) -> Result<T, DafErrorCode>
where
    F: FnOnce() -> T,
{
    match panic::catch_unwind(panic::AssertUnwindSafe(f)) {
        Ok(v) => Ok(v),
        Err(_) => {
            set_last_error("panic crossed FFI boundary");
            Err(DafErrorCode::InternalError)
        }
    }
}

#[no_mangle]
pub extern "C" fn daf_query(
    ptr: *mut DataAccess,
    resource_id: *const c_char,
    user_id: *const c_char,
) -> c_int {
    clear_last_error();
    if ptr.is_null() {
        set_last_error("null DataAccess pointer");
        return DafErrorCode::InvalidArgument as c_int;
    }
    let result = catch_panic(|| {
        let rid = validate_utf8_cstr(resource_id, "resource_id")?;
        let uid_str = if user_id.is_null() {
            None
        } else {
            Some(validate_utf8_cstr(user_id, "user_id")?.to_string())
        };
        let daf = unsafe { &*ptr };
        block_on(async move {
            let info = QueryInfo {
                resource_id: ResourceId::new(rid),
                filters: None,
                algorithm: None,
            };
            let uid_ref = uid_str.as_ref().map(UserId::new);
            (*daf).query(info, uid_ref.as_ref()).await
        })
    });

    match result {
        Ok(Ok(_)) => DafErrorCode::Ok as c_int,
        Ok(Err(e)) => {
            set_last_error(&e.to_string());
            map_data_access_error(e) as c_int
        }
        Err(code) => code as c_int,
    }
}

#[no_mangle]
pub extern "C" fn daf_post(
    ptr: *mut DataAccess,
    resource_type: *const c_char,
    user_id: *const c_char,
) -> c_int {
    clear_last_error();
    if ptr.is_null() {
        set_last_error("null DataAccess pointer");
        return DafErrorCode::InvalidArgument as c_int;
    }
    let result = catch_panic(|| {
        let resource_type = validate_utf8_cstr(resource_type, "resource_type")?;
        let uid_str = if user_id.is_null() {
            None
        } else {
            Some(validate_utf8_cstr(user_id, "user_id")?.to_string())
        };
        let daf = unsafe { &*ptr };
        let info = PostInfo {
            resource_type: resource_type.to_string(),
            data: HashMap::new(),
        };
        let uid_ref = uid_str.as_ref().map(UserId::new);
        block_on(async move { (*daf).post(info, uid_ref.as_ref()).await })
    });

    match result {
        Ok(Ok(_)) => DafErrorCode::Ok as c_int,
        Ok(Err(e)) => {
            set_last_error(&e.to_string());
            map_data_access_error(e) as c_int
        }
        Err(code) => code as c_int,
    }
}

#[no_mangle]
pub extern "C" fn daf_put(
    ptr: *mut DataAccess,
    resource_id: *const c_char,
    user_id: *const c_char,
) -> c_int {
    clear_last_error();
    if ptr.is_null() {
        set_last_error("null DataAccess pointer");
        return DafErrorCode::InvalidArgument as c_int;
    }
    let result = catch_panic(|| {
        let rid = validate_utf8_cstr(resource_id, "resource_id")?;
        let uid_str = if user_id.is_null() {
            None
        } else {
            Some(validate_utf8_cstr(user_id, "user_id")?.to_string())
        };
        let daf = unsafe { &*ptr };
        let info = PutInfo {
            resource_id: ResourceId::new(rid),
            data: HashMap::new(),
        };
        let uid_ref = uid_str.as_ref().map(UserId::new);
        block_on(async move { (*daf).put(info, uid_ref.as_ref()).await })
    });

    match result {
        Ok(Ok(_)) => DafErrorCode::Ok as c_int,
        Ok(Err(e)) => {
            set_last_error(&e.to_string());
            map_data_access_error(e) as c_int
        }
        Err(code) => code as c_int,
    }
}

#[no_mangle]
pub extern "C" fn daf_delete(
    ptr: *mut DataAccess,
    resource_id: *const c_char,
    user_id: *const c_char,
) -> c_int {
    clear_last_error();
    if ptr.is_null() {
        set_last_error("null DataAccess pointer");
        return DafErrorCode::InvalidArgument as c_int;
    }
    let result = catch_panic(|| {
        let rid = validate_utf8_cstr(resource_id, "resource_id")?;
        let uid_str = if user_id.is_null() {
            None
        } else {
            Some(validate_utf8_cstr(user_id, "user_id")?.to_string())
        };
        let daf = unsafe { &*ptr };
        let info = DeleteInfo {
            resource_id: ResourceId::new(rid),
        };
        let uid_ref = uid_str.as_ref().map(UserId::new);
        block_on(async move { (*daf).delete(info, uid_ref.as_ref()).await })
    });

    match result {
        Ok(Ok(_)) => DafErrorCode::Ok as c_int,
        Ok(Err(e)) => {
            set_last_error(&e.to_string());
            map_data_access_error(e) as c_int
        }
        Err(code) => code as c_int,
    }
}

#[no_mangle]
pub extern "C" fn daf_last_error_message() -> *const c_char {
    LAST_ERROR.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |s| s.as_ptr())
    })
}
