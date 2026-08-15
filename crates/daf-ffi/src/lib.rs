#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::panic;
use std::sync::{Arc, OnceLock};

use daf_algorithms::FibonacciDP;
use daf_application::DataAccess;
use daf_cache::MemoryCache;
use daf_core::{
    DataAccessError, DeleteInfo, JsonValue, PostInfo, PutInfo, QueryInfo, ResourceId, UserId,
    ValidationError,
};
use daf_repository::MemoryRepository;

static mut LAST_ERROR: Option<CString> = None;

fn set_last_error(msg: &str) {
    unsafe {
        LAST_ERROR = Some(CString::new(msg).unwrap());
    }
}

fn clear_last_error() {
    unsafe {
        LAST_ERROR = None;
    }
}

#[repr(C)]
pub enum DafErrorCode {
    Ok = 0,
    InvalidArgument = 1,
    NotFound = 2,
    AuthorizationFailed = 3,
    ValidationFailed = 4,
    InternalError = 5,
}

fn map_data_access_error(err: DataAccessError) -> DafErrorCode {
    match err {
        DataAccessError::NotFound(_) => DafErrorCode::NotFound,
        DataAccessError::Authorization(_) => DafErrorCode::AuthorizationFailed,
        DataAccessError::Validation(_) => DafErrorCode::ValidationFailed,
        _ => DafErrorCode::InternalError,
    }
}

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("failed to create tokio runtime"))
}

fn block_on<F>(f: F) -> F::Output
where
    F: std::future::Future,
{
    runtime().block_on(f)
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
        Box::into_raw(Box::new(daf))
    }) {
        Ok(ptr) => ptr,
        Err(_) => {
            set_last_error("panic in daf_data_access_new");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn daf_data_access_free(ptr: *mut DataAccess) {
    if ptr.is_null() {
        return;
    }
    unsafe { drop(Box::from_raw(ptr)) };
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
    let result = catch_panic(|| {
        let daf = panic::AssertUnwindSafe(unsafe { &*ptr });
        let rid = unsafe { CStr::from_ptr(resource_id) }
            .to_str()
            .map_err(|_| {
                DataAccessError::Validation(ValidationError::new("invalid resource_id utf8"))
            })?;
        let uid = if user_id.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(user_id) }.to_str().map_err(|_| {
                DataAccessError::Validation(ValidationError::new("invalid user_id utf8"))
            })?)
        };
        block_on(async move {
            let info = QueryInfo {
                resource_id: ResourceId::new(rid),
                filters: None,
                algorithm: None,
            };
            (*daf).query(info, uid.map(UserId::new).as_ref()).await
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
    let result = catch_panic(|| {
        let daf = panic::AssertUnwindSafe(unsafe { &*ptr });
        let resource_type = unsafe { CStr::from_ptr(resource_type) }
            .to_str()
            .map_err(|_| {
                DataAccessError::Validation(ValidationError::new("invalid resource_type utf8"))
            })?;
        let uid = if user_id.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(user_id) }.to_str().map_err(|_| {
                DataAccessError::Validation(ValidationError::new("invalid user_id utf8"))
            })?)
        };
        let info = PostInfo {
            resource_type: resource_type.to_string(),
            data: HashMap::new(),
        };
        block_on(async move { (*daf).post(info, uid.map(UserId::new).as_ref()).await })
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
    let result = catch_panic(|| {
        let daf = panic::AssertUnwindSafe(unsafe { &*ptr });
        let rid = unsafe { CStr::from_ptr(resource_id) }
            .to_str()
            .map_err(|_| {
                DataAccessError::Validation(ValidationError::new("invalid resource_id utf8"))
            })?;
        let uid = if user_id.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(user_id) }.to_str().map_err(|_| {
                DataAccessError::Validation(ValidationError::new("invalid user_id utf8"))
            })?)
        };
        let info = PutInfo {
            resource_id: ResourceId::new(rid),
            data: HashMap::new(),
        };
        block_on(async move { (*daf).put(info, uid.map(UserId::new).as_ref()).await })
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
    let result = catch_panic(|| {
        let daf = panic::AssertUnwindSafe(unsafe { &*ptr });
        let rid = unsafe { CStr::from_ptr(resource_id) }
            .to_str()
            .map_err(|_| {
                DataAccessError::Validation(ValidationError::new("invalid resource_id utf8"))
            })?;
        let uid = if user_id.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(user_id) }.to_str().map_err(|_| {
                DataAccessError::Validation(ValidationError::new("invalid user_id utf8"))
            })?)
        };
        let info = DeleteInfo {
            resource_id: ResourceId::new(rid),
        };
        block_on(async move { (*daf).delete(info, uid.map(UserId::new).as_ref()).await })
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
    unsafe { LAST_ERROR.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()) }
}
