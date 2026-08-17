use std::ffi::c_int;

use daf_ffi::{daf_data_access_free, daf_data_access_new, DafErrorCode};

#[test]
fn ffi_double_free_returns_invalid_argument() {
    let ptr = daf_data_access_new();
    assert!(!ptr.is_null());

    let rc1 = daf_data_access_free(ptr);
    assert_eq!(rc1, DafErrorCode::Ok as c_int);

    let rc2 = daf_data_access_free(ptr);
    assert_eq!(rc2, DafErrorCode::InvalidArgument as c_int);
}
