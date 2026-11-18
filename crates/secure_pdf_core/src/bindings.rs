#[inline]
pub unsafe fn AbortDoc(hdc: HDC) -> i32 {
    windows_core::link!("gdi32.dll" "system" fn AbortDoc(hdc : HDC) -> i32);
    unsafe { AbortDoc(hdc) }
}
#[inline]
pub unsafe fn BCryptCloseAlgorithmProvider(
    halgorithm: BCRYPT_ALG_HANDLE,
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptCloseAlgorithmProvider(halgorithm : BCRYPT_ALG_HANDLE, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe { BCryptCloseAlgorithmProvider(halgorithm as _, dwflags) }
}
#[inline]
pub unsafe fn BCryptCreateHash(
    halgorithm: BCRYPT_ALG_HANDLE,
    phhash: *mut BCRYPT_HASH_HANDLE,
    pbhashobject: Option<*mut u8>,
    cbhashobject: u32,
    pbsecret: Option<&[u8]>,
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptCreateHash(halgorithm : BCRYPT_ALG_HANDLE, phhash : *mut BCRYPT_HASH_HANDLE, pbhashobject : *mut u8, cbhashobject : u32, pbsecret : *const u8, cbsecret : u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptCreateHash(
            halgorithm as _,
            phhash as _,
            pbhashobject.unwrap_or(core::mem::zeroed()) as _,
            cbhashobject,
            pbsecret.map_or(core::ptr::null(), |slice| slice.as_ptr()),
            pbsecret.map_or(0, |slice| slice.len().try_into().unwrap()),
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptDecrypt(
    hkey: BCRYPT_KEY_HANDLE,
    pbinput: Option<&[u8]>,
    ppaddinginfo: Option<*const core::ffi::c_void>,
    pbiv: Option<&mut [u8]>,
    pboutput: Option<*mut u8>,
    cboutput: u32,
    pcbresult: *mut u32,
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptDecrypt(hkey : BCRYPT_KEY_HANDLE, pbinput : *const u8, cbinput : u32, ppaddinginfo : *const core::ffi::c_void, pbiv : *mut u8, cbiv : u32, pboutput : *mut u8, cboutput : u32, pcbresult : *mut u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptDecrypt(
            hkey as _,
            pbinput.map_or(core::ptr::null(), |slice| slice.as_ptr()),
            pbinput.map_or(0, |slice| slice.len().try_into().unwrap()),
            ppaddinginfo.unwrap_or(core::mem::zeroed()) as _,
            pbiv.as_deref()
                .map_or(core::ptr::null_mut(), |slice| slice.as_ptr().cast_mut()),
            pbiv.as_deref()
                .map_or(0, |slice| slice.len().try_into().unwrap()),
            pboutput.unwrap_or(core::mem::zeroed()) as _,
            cboutput,
            pcbresult as _,
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptDeriveKeyPBKDF2(
    hprf: BCRYPT_ALG_HANDLE,
    pbpassword: Option<&[u8]>,
    pbsalt: Option<&[u8]>,
    citerations: u64,
    pbderivedkey: *mut u8,
    cbderivedkey: u32,
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptDeriveKeyPBKDF2(hprf : BCRYPT_ALG_HANDLE, pbpassword : *const u8, cbpassword : u32, pbsalt : *const u8, cbsalt : u32, citerations : u64, pbderivedkey : *mut u8, cbderivedkey : u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptDeriveKeyPBKDF2(
            hprf,
            pbpassword.map_or(core::ptr::null(), |slice| slice.as_ptr()),
            pbpassword.map_or(0, |slice| slice.len().try_into().unwrap()),
            pbsalt.map_or(core::ptr::null(), |slice| slice.as_ptr()),
            pbsalt.map_or(0, |slice| slice.len().try_into().unwrap()),
            citerations,
            pbderivedkey as _,
            cbderivedkey,
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptDestroyHash(hhash: BCRYPT_HASH_HANDLE) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptDestroyHash(hhash : BCRYPT_HASH_HANDLE) -> windows_core::NTSTATUS);
    unsafe { BCryptDestroyHash(hhash as _) }
}
#[inline]
pub unsafe fn BCryptDestroyKey(hkey: BCRYPT_KEY_HANDLE) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptDestroyKey(hkey : BCRYPT_KEY_HANDLE) -> windows_core::NTSTATUS);
    unsafe { BCryptDestroyKey(hkey as _) }
}
#[inline]
pub unsafe fn BCryptEncrypt(
    hkey: BCRYPT_KEY_HANDLE,
    pbinput: Option<&[u8]>,
    ppaddinginfo: Option<*const core::ffi::c_void>,
    pbiv: Option<&mut [u8]>,
    pboutput: Option<*mut u8>,
    cboutput: u32,
    pcbresult: *mut u32,
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptEncrypt(hkey : BCRYPT_KEY_HANDLE, pbinput : *const u8, cbinput : u32, ppaddinginfo : *const core::ffi::c_void, pbiv : *mut u8, cbiv : u32, pboutput : *mut u8, cboutput : u32, pcbresult : *mut u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptEncrypt(
            hkey as _,
            pbinput.map_or(core::ptr::null(), |slice| slice.as_ptr()),
            pbinput.map_or(0, |slice| slice.len().try_into().unwrap()),
            ppaddinginfo.unwrap_or(core::mem::zeroed()) as _,
            pbiv.as_deref()
                .map_or(core::ptr::null_mut(), |slice| slice.as_ptr().cast_mut()),
            pbiv.as_deref()
                .map_or(0, |slice| slice.len().try_into().unwrap()),
            pboutput.unwrap_or(core::mem::zeroed()) as _,
            cboutput,
            pcbresult as _,
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptFinishHash(
    hhash: BCRYPT_HASH_HANDLE,
    pboutput: *mut u8,
    cboutput: u32,
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptFinishHash(hhash : BCRYPT_HASH_HANDLE, pboutput : *mut u8, cboutput : u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe { BCryptFinishHash(hhash as _, pboutput as _, cboutput, dwflags) }
}
#[inline]
pub unsafe fn BCryptGenRandom(
    halgorithm: Option<BCRYPT_ALG_HANDLE>,
    pbbuffer: *mut u8,
    cbbuffer: u32,
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptGenRandom(halgorithm : BCRYPT_ALG_HANDLE, pbbuffer : *mut u8, cbbuffer : u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptGenRandom(
            halgorithm.unwrap_or(core::mem::zeroed()) as _,
            pbbuffer as _,
            cbbuffer,
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptGenerateSymmetricKey(
    halgorithm: BCRYPT_ALG_HANDLE,
    phkey: *mut BCRYPT_KEY_HANDLE,
    pbkeyobject: Option<*mut u8>,
    cbkeyobject: u32,
    pbsecret: &[u8],
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptGenerateSymmetricKey(halgorithm : BCRYPT_ALG_HANDLE, phkey : *mut BCRYPT_KEY_HANDLE, pbkeyobject : *mut u8, cbkeyobject : u32, pbsecret : *const u8, cbsecret : u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptGenerateSymmetricKey(
            halgorithm as _,
            phkey as _,
            pbkeyobject.unwrap_or(core::mem::zeroed()) as _,
            cbkeyobject,
            pbsecret.as_ptr(),
            pbsecret.len().try_into().unwrap(),
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptGetProperty<P1>(
    hobject: BCRYPT_HANDLE,
    pszproperty: P1,
    pboutput: Option<*mut u8>,
    cboutput: u32,
    pcbresult: *mut u32,
    dwflags: u32,
) -> windows_core::NTSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("bcrypt.dll" "system" fn BCryptGetProperty(hobject : BCRYPT_HANDLE, pszproperty : windows_core::PCWSTR, pboutput : *mut u8, cboutput : u32, pcbresult : *mut u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptGetProperty(
            hobject,
            pszproperty.param().abi(),
            pboutput.unwrap_or(core::mem::zeroed()) as _,
            cboutput,
            pcbresult as _,
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptHashData(
    hhash: BCRYPT_HASH_HANDLE,
    pbinput: &[u8],
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptHashData(hhash : BCRYPT_HASH_HANDLE, pbinput : *const u8, cbinput : u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptHashData(
            hhash as _,
            pbinput.as_ptr(),
            pbinput.len().try_into().unwrap(),
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptOpenAlgorithmProvider<P1, P2>(
    phalgorithm: *mut BCRYPT_ALG_HANDLE,
    pszalgid: P1,
    pszimplementation: P2,
    dwflags: u32,
) -> windows_core::NTSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
    P2: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("bcrypt.dll" "system" fn BCryptOpenAlgorithmProvider(phalgorithm : *mut BCRYPT_ALG_HANDLE, pszalgid : windows_core::PCWSTR, pszimplementation : windows_core::PCWSTR, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptOpenAlgorithmProvider(
            phalgorithm as _,
            pszalgid.param().abi(),
            pszimplementation.param().abi(),
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptSetProperty<P1>(
    hobject: BCRYPT_HANDLE,
    pszproperty: P1,
    pbinput: &[u8],
    dwflags: u32,
) -> windows_core::NTSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("bcrypt.dll" "system" fn BCryptSetProperty(hobject : BCRYPT_HANDLE, pszproperty : windows_core::PCWSTR, pbinput : *const u8, cbinput : u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptSetProperty(
            hobject as _,
            pszproperty.param().abi(),
            pbinput.as_ptr(),
            pbinput.len().try_into().unwrap(),
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn CloseHandle(hobject: HANDLE) -> windows_core::BOOL {
    windows_core::link!("kernel32.dll" "system" fn CloseHandle(hobject : HANDLE) -> windows_core::BOOL);
    unsafe { CloseHandle(hobject) }
}
#[inline]
pub unsafe fn CoCreateInstance<P1, T>(
    rclsid: *const windows_core::GUID,
    punkouter: P1,
    dwclscontext: u32,
) -> windows_core::Result<T>
where
    P1: windows_core::Param<windows_core::IUnknown>,
    T: windows_core::Interface,
{
    windows_core::link!("ole32.dll" "system" fn CoCreateInstance(rclsid : *const windows_core::GUID, punkouter : *mut core::ffi::c_void, dwclscontext : u32, riid : *const windows_core::GUID, ppv : *mut *mut core::ffi::c_void) -> windows_core::HRESULT);
    let mut result__ = core::ptr::null_mut();
    unsafe {
        CoCreateInstance(
            rclsid,
            punkouter.param().abi(),
            dwclscontext,
            &T::IID,
            &mut result__,
        )
        .and_then(|| windows_core::imp::Type::from_abi(result__))
    }
}
#[inline]
pub unsafe fn CoInitializeEx(
    pvreserved: Option<*const core::ffi::c_void>,
    dwcoinit: u32,
) -> windows_core::HRESULT {
    windows_core::link!("ole32.dll" "system" fn CoInitializeEx(pvreserved : *const core::ffi::c_void, dwcoinit : u32) -> windows_core::HRESULT);
    unsafe { CoInitializeEx(pvreserved.unwrap_or(core::mem::zeroed()) as _, dwcoinit) }
}
#[inline]
pub unsafe fn CoSetProxyBlanket<P0>(
    pproxy: P0,
    dwauthnsvc: u32,
    dwauthzsvc: u32,
    pserverprincname: Option<*const OLECHAR>,
    dwauthnlevel: u32,
    dwimplevel: u32,
    pauthinfo: Option<RPC_AUTH_IDENTITY_HANDLE>,
    dwcapabilities: u32,
) -> windows_core::HRESULT
where
    P0: windows_core::Param<windows_core::IUnknown>,
{
    windows_core::link!("ole32.dll" "system" fn CoSetProxyBlanket(pproxy : *mut core::ffi::c_void, dwauthnsvc : u32, dwauthzsvc : u32, pserverprincname : *const OLECHAR, dwauthnlevel : u32, dwimplevel : u32, pauthinfo : RPC_AUTH_IDENTITY_HANDLE, dwcapabilities : u32) -> windows_core::HRESULT);
    unsafe {
        CoSetProxyBlanket(
            pproxy.param().abi(),
            dwauthnsvc,
            dwauthzsvc,
            pserverprincname.unwrap_or(core::mem::zeroed()) as _,
            dwauthnlevel,
            dwimplevel,
            pauthinfo.unwrap_or(core::mem::zeroed()) as _,
            dwcapabilities,
        )
    }
}
#[inline]
pub unsafe fn CoUninitialize() {
    windows_core::link!("ole32.dll" "system" fn CoUninitialize());
    unsafe { CoUninitialize() }
}
#[inline]
pub unsafe fn CreateDCW<P0, P1, P2>(
    pwszdriver: P0,
    pwszdevice: P1,
    pszport: P2,
    pdm: Option<*const DEVMODEW>,
) -> HDC
where
    P0: windows_core::Param<windows_core::PCWSTR>,
    P1: windows_core::Param<windows_core::PCWSTR>,
    P2: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("gdi32.dll" "system" fn CreateDCW(pwszdriver : windows_core::PCWSTR, pwszdevice : windows_core::PCWSTR, pszport : windows_core::PCWSTR, pdm : *const DEVMODEW) -> HDC);
    unsafe {
        CreateDCW(
            pwszdriver.param().abi(),
            pwszdevice.param().abi(),
            pszport.param().abi(),
            pdm.unwrap_or(core::mem::zeroed()) as _,
        )
    }
}
#[inline]
pub unsafe fn CreateFontW<P13>(
    cheight: i32,
    cwidth: i32,
    cescapement: i32,
    corientation: i32,
    cweight: i32,
    bitalic: u32,
    bunderline: u32,
    bstrikeout: u32,
    icharset: u32,
    ioutprecision: u32,
    iclipprecision: u32,
    iquality: u32,
    ipitchandfamily: u32,
    pszfacename: P13,
) -> HFONT
where
    P13: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("gdi32.dll" "system" fn CreateFontW(cheight : i32, cwidth : i32, cescapement : i32, corientation : i32, cweight : i32, bitalic : u32, bunderline : u32, bstrikeout : u32, icharset : u32, ioutprecision : u32, iclipprecision : u32, iquality : u32, ipitchandfamily : u32, pszfacename : windows_core::PCWSTR) -> HFONT);
    unsafe {
        CreateFontW(
            cheight,
            cwidth,
            cescapement,
            corientation,
            cweight,
            bitalic,
            bunderline,
            bstrikeout,
            icharset,
            ioutprecision,
            iclipprecision,
            iquality,
            ipitchandfamily,
            pszfacename.param().abi(),
        )
    }
}
#[inline]
pub unsafe fn CreateMutexW<P2>(
    lpmutexattributes: Option<*const SECURITY_ATTRIBUTES>,
    binitialowner: bool,
    lpname: P2,
) -> HANDLE
where
    P2: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("kernel32.dll" "system" fn CreateMutexW(lpmutexattributes : *const SECURITY_ATTRIBUTES, binitialowner : windows_core::BOOL, lpname : windows_core::PCWSTR) -> HANDLE);
    unsafe {
        CreateMutexW(
            lpmutexattributes.unwrap_or(core::mem::zeroed()) as _,
            binitialowner.into(),
            lpname.param().abi(),
        )
    }
}
#[inline]
pub unsafe fn CryptProtectData<P1>(
    pdatain: *const DATA_BLOB,
    szdatadescr: P1,
    poptionalentropy: Option<*const DATA_BLOB>,
    pvreserved: Option<*const core::ffi::c_void>,
    ppromptstruct: Option<*const CRYPTPROTECT_PROMPTSTRUCT>,
    dwflags: u32,
    pdataout: *mut DATA_BLOB,
) -> windows_core::BOOL
where
    P1: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("crypt32.dll" "system" fn CryptProtectData(pdatain : *const DATA_BLOB, szdatadescr : windows_core::PCWSTR, poptionalentropy : *const DATA_BLOB, pvreserved : *const core::ffi::c_void, ppromptstruct : *const CRYPTPROTECT_PROMPTSTRUCT, dwflags : u32, pdataout : *mut DATA_BLOB) -> windows_core::BOOL);
    unsafe {
        CryptProtectData(
            pdatain,
            szdatadescr.param().abi(),
            poptionalentropy.unwrap_or(core::mem::zeroed()) as _,
            pvreserved.unwrap_or(core::mem::zeroed()) as _,
            ppromptstruct.unwrap_or(core::mem::zeroed()) as _,
            dwflags,
            pdataout as _,
        )
    }
}
#[inline]
pub unsafe fn CryptUnprotectData(
    pdatain: *const DATA_BLOB,
    ppszdatadescr: *mut windows_core::PWSTR,
    poptionalentropy: Option<*const DATA_BLOB>,
    pvreserved: Option<*const core::ffi::c_void>,
    ppromptstruct: Option<*const CRYPTPROTECT_PROMPTSTRUCT>,
    dwflags: u32,
    pdataout: *mut DATA_BLOB,
) -> windows_core::BOOL {
    windows_core::link!("crypt32.dll" "system" fn CryptUnprotectData(pdatain : *const DATA_BLOB, ppszdatadescr : *mut windows_core::PWSTR, poptionalentropy : *const DATA_BLOB, pvreserved : *const core::ffi::c_void, ppromptstruct : *const CRYPTPROTECT_PROMPTSTRUCT, dwflags : u32, pdataout : *mut DATA_BLOB) -> windows_core::BOOL);
    unsafe {
        CryptUnprotectData(
            pdatain,
            ppszdatadescr as _,
            poptionalentropy.unwrap_or(core::mem::zeroed()) as _,
            pvreserved.unwrap_or(core::mem::zeroed()) as _,
            ppromptstruct.unwrap_or(core::mem::zeroed()) as _,
            dwflags,
            pdataout as _,
        )
    }
}
#[inline]
pub unsafe fn DeleteDC(hdc: HDC) -> windows_core::BOOL {
    windows_core::link!("gdi32.dll" "system" fn DeleteDC(hdc : HDC) -> windows_core::BOOL);
    unsafe { DeleteDC(hdc) }
}
#[inline]
pub unsafe fn DeleteObject(ho: HGDIOBJ) -> windows_core::BOOL {
    windows_core::link!("gdi32.dll" "system" fn DeleteObject(ho : HGDIOBJ) -> windows_core::BOOL);
    unsafe { DeleteObject(ho) }
}
#[inline]
pub unsafe fn EndDoc(hdc: HDC) -> i32 {
    windows_core::link!("gdi32.dll" "system" fn EndDoc(hdc : HDC) -> i32);
    unsafe { EndDoc(hdc) }
}
#[inline]
pub unsafe fn EndPage(hdc: HDC) -> i32 {
    windows_core::link!("gdi32.dll" "system" fn EndPage(hdc : HDC) -> i32);
    unsafe { EndPage(hdc) }
}
#[inline]
pub unsafe fn EnumPortsW<P0>(
    pname: P0,
    level: u32,
    pport: Option<*mut u8>,
    cbbuf: u32,
    pcbneeded: *mut u32,
    pcreturned: *mut u32,
) -> windows_core::BOOL
where
    P0: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("winspool.drv" "system" fn EnumPortsW(pname : windows_core::PCWSTR, level : u32, pport : *mut u8, cbbuf : u32, pcbneeded : *mut u32, pcreturned : *mut u32) -> windows_core::BOOL);
    unsafe {
        EnumPortsW(
            pname.param().abi(),
            level,
            pport.unwrap_or(core::mem::zeroed()) as _,
            cbbuf,
            pcbneeded as _,
            pcreturned as _,
        )
    }
}
#[inline]
pub unsafe fn EnumPrintersW<P1>(
    flags: u32,
    name: P1,
    level: u32,
    pprinterenum: Option<*mut u8>,
    cbbuf: u32,
    pcbneeded: *mut u32,
    pcreturned: *mut u32,
) -> windows_core::BOOL
where
    P1: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("winspool.drv" "system" fn EnumPrintersW(flags : u32, name : windows_core::PCWSTR, level : u32, pprinterenum : *mut u8, cbbuf : u32, pcbneeded : *mut u32, pcreturned : *mut u32) -> windows_core::BOOL);
    unsafe {
        EnumPrintersW(
            flags,
            name.param().abi(),
            level,
            pprinterenum.unwrap_or(core::mem::zeroed()) as _,
            cbbuf,
            pcbneeded as _,
            pcreturned as _,
        )
    }
}
#[inline]
pub unsafe fn FreeLibrary(hlibmodule: HMODULE) -> windows_core::BOOL {
    windows_core::link!("kernel32.dll" "system" fn FreeLibrary(hlibmodule : HMODULE) -> windows_core::BOOL);
    unsafe { FreeLibrary(hlibmodule) }
}
#[inline]
pub unsafe fn GetAdaptersAddresses(
    family: u32,
    flags: u32,
    reserved: Option<*const core::ffi::c_void>,
    adapteraddresses: Option<*mut IP_ADAPTER_ADDRESSES_LH>,
    sizepointer: *mut u32,
) -> u32 {
    windows_core::link!("iphlpapi.dll" "system" fn GetAdaptersAddresses(family : u32, flags : u32, reserved : *const core::ffi::c_void, adapteraddresses : *mut IP_ADAPTER_ADDRESSES_LH, sizepointer : *mut u32) -> u32);
    unsafe {
        GetAdaptersAddresses(
            family,
            flags,
            reserved.unwrap_or(core::mem::zeroed()) as _,
            adapteraddresses.unwrap_or(core::mem::zeroed()) as _,
            sizepointer as _,
        )
    }
}
#[inline]
pub unsafe fn GetComputerNameExW(
    nametype: COMPUTER_NAME_FORMAT,
    lpbuffer: Option<windows_core::PWSTR>,
    nsize: *mut u32,
) -> windows_core::BOOL {
    windows_core::link!("kernel32.dll" "system" fn GetComputerNameExW(nametype : COMPUTER_NAME_FORMAT, lpbuffer : windows_core::PWSTR, nsize : *mut u32) -> windows_core::BOOL);
    unsafe {
        GetComputerNameExW(
            nametype,
            lpbuffer.unwrap_or(core::mem::zeroed()) as _,
            nsize as _,
        )
    }
}
#[inline]
pub unsafe fn GetDefaultPrinterW(
    pszbuffer: Option<windows_core::PWSTR>,
    pcchbuffer: *mut u32,
) -> windows_core::BOOL {
    windows_core::link!("winspool.drv" "system" fn GetDefaultPrinterW(pszbuffer : windows_core::PWSTR, pcchbuffer : *mut u32) -> windows_core::BOOL);
    unsafe {
        GetDefaultPrinterW(
            pszbuffer.unwrap_or(core::mem::zeroed()) as _,
            pcchbuffer as _,
        )
    }
}
#[inline]
pub unsafe fn GetDeviceCaps(hdc: Option<HDC>, index: i32) -> i32 {
    windows_core::link!("gdi32.dll" "system" fn GetDeviceCaps(hdc : HDC, index : i32) -> i32);
    unsafe { GetDeviceCaps(hdc.unwrap_or(core::mem::zeroed()) as _, index) }
}
#[inline]
pub unsafe fn GetLastError() -> u32 {
    windows_core::link!("kernel32.dll" "system" fn GetLastError() -> u32);
    unsafe { GetLastError() }
}
#[inline]
pub unsafe fn GetProcAddress<P1>(hmodule: HMODULE, lpprocname: P1) -> FARPROC
where
    P1: windows_core::Param<windows_core::PCSTR>,
{
    windows_core::link!("kernel32.dll" "system" fn GetProcAddress(hmodule : HMODULE, lpprocname : windows_core::PCSTR) -> FARPROC);
    unsafe { GetProcAddress(hmodule, lpprocname.param().abi()) }
}
#[inline]
pub unsafe fn GetVolumeInformationW<P0>(
    lprootpathname: P0,
    lpvolumenamebuffer: Option<windows_core::PWSTR>,
    nvolumenamesize: u32,
    lpvolumeserialnumber: Option<*mut u32>,
    lpmaximumcomponentlength: Option<*mut u32>,
    lpfilesystemflags: Option<*mut u32>,
    lpfilesystemnamebuffer: Option<windows_core::PWSTR>,
    nfilesystemnamesize: u32,
) -> windows_core::BOOL
where
    P0: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("kernel32.dll" "system" fn GetVolumeInformationW(lprootpathname : windows_core::PCWSTR, lpvolumenamebuffer : windows_core::PWSTR, nvolumenamesize : u32, lpvolumeserialnumber : *mut u32, lpmaximumcomponentlength : *mut u32, lpfilesystemflags : *mut u32, lpfilesystemnamebuffer : windows_core::PWSTR, nfilesystemnamesize : u32) -> windows_core::BOOL);
    unsafe {
        GetVolumeInformationW(
            lprootpathname.param().abi(),
            lpvolumenamebuffer.unwrap_or(core::mem::zeroed()) as _,
            nvolumenamesize,
            lpvolumeserialnumber.unwrap_or(core::mem::zeroed()) as _,
            lpmaximumcomponentlength.unwrap_or(core::mem::zeroed()) as _,
            lpfilesystemflags.unwrap_or(core::mem::zeroed()) as _,
            lpfilesystemnamebuffer.unwrap_or(core::mem::zeroed()) as _,
            nfilesystemnamesize,
        )
    }
}
#[inline]
pub unsafe fn GetWindowsDirectoryW(lpbuffer: Option<windows_core::PWSTR>, usize: u32) -> u32 {
    windows_core::link!("kernel32.dll" "system" fn GetWindowsDirectoryW(lpbuffer : windows_core::PWSTR, usize : u32) -> u32);
    unsafe { GetWindowsDirectoryW(lpbuffer.unwrap_or(core::mem::zeroed()) as _, usize) }
}
#[inline]
pub unsafe fn LoadLibraryW<P0>(lplibfilename: P0) -> HMODULE
where
    P0: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("kernel32.dll" "system" fn LoadLibraryW(lplibfilename : windows_core::PCWSTR) -> HMODULE);
    unsafe { LoadLibraryW(lplibfilename.param().abi()) }
}
#[inline]
pub unsafe fn LocalFree(hmem: HLOCAL) -> HLOCAL {
    windows_core::link!("kernel32.dll" "system" fn LocalFree(hmem : HLOCAL) -> HLOCAL);
    unsafe { LocalFree(hmem) }
}
#[inline]
pub unsafe fn ModifyWorldTransform(
    hdc: HDC,
    lpxf: Option<*const XFORM>,
    mode: u32,
) -> windows_core::BOOL {
    windows_core::link!("gdi32.dll" "system" fn ModifyWorldTransform(hdc : HDC, lpxf : *const XFORM, mode : u32) -> windows_core::BOOL);
    unsafe { ModifyWorldTransform(hdc, lpxf.unwrap_or(core::mem::zeroed()) as _, mode) }
}
#[inline]
pub unsafe fn RegCloseKey(hkey: HKEY) -> LSTATUS {
    windows_core::link!("advapi32.dll" "system" fn RegCloseKey(hkey : HKEY) -> LSTATUS);
    unsafe { RegCloseKey(hkey) }
}
#[inline]
pub unsafe fn RegCreateKeyExW<P1, P3>(
    hkey: HKEY,
    lpsubkey: P1,
    reserved: Option<u32>,
    lpclass: P3,
    dwoptions: u32,
    samdesired: REGSAM,
    lpsecurityattributes: Option<*const SECURITY_ATTRIBUTES>,
    phkresult: *mut HKEY,
    lpdwdisposition: Option<*mut u32>,
) -> LSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
    P3: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("advapi32.dll" "system" fn RegCreateKeyExW(hkey : HKEY, lpsubkey : windows_core::PCWSTR, reserved : u32, lpclass : windows_core::PCWSTR, dwoptions : u32, samdesired : REGSAM, lpsecurityattributes : *const SECURITY_ATTRIBUTES, phkresult : *mut HKEY, lpdwdisposition : *mut u32) -> LSTATUS);
    unsafe {
        RegCreateKeyExW(
            hkey,
            lpsubkey.param().abi(),
            reserved.unwrap_or(core::mem::zeroed()) as _,
            lpclass.param().abi(),
            dwoptions,
            samdesired,
            lpsecurityattributes.unwrap_or(core::mem::zeroed()) as _,
            phkresult as _,
            lpdwdisposition.unwrap_or(core::mem::zeroed()) as _,
        )
    }
}
#[inline]
pub unsafe fn RegDeleteValueW<P1>(hkey: HKEY, lpvaluename: P1) -> LSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("advapi32.dll" "system" fn RegDeleteValueW(hkey : HKEY, lpvaluename : windows_core::PCWSTR) -> LSTATUS);
    unsafe { RegDeleteValueW(hkey, lpvaluename.param().abi()) }
}
#[inline]
pub unsafe fn RegOpenKeyExW<P1>(
    hkey: HKEY,
    lpsubkey: P1,
    uloptions: Option<u32>,
    samdesired: REGSAM,
    phkresult: *mut HKEY,
) -> LSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("advapi32.dll" "system" fn RegOpenKeyExW(hkey : HKEY, lpsubkey : windows_core::PCWSTR, uloptions : u32, samdesired : REGSAM, phkresult : *mut HKEY) -> LSTATUS);
    unsafe {
        RegOpenKeyExW(
            hkey,
            lpsubkey.param().abi(),
            uloptions.unwrap_or(core::mem::zeroed()) as _,
            samdesired,
            phkresult as _,
        )
    }
}
#[inline]
pub unsafe fn RegQueryValueExW<P1>(
    hkey: HKEY,
    lpvaluename: P1,
    lpreserved: Option<*const u32>,
    lptype: Option<*mut u32>,
    lpdata: Option<*mut u8>,
    lpcbdata: *mut u32,
) -> LSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("advapi32.dll" "system" fn RegQueryValueExW(hkey : HKEY, lpvaluename : windows_core::PCWSTR, lpreserved : *const u32, lptype : *mut u32, lpdata : *mut u8, lpcbdata : *mut u32) -> LSTATUS);
    unsafe {
        RegQueryValueExW(
            hkey,
            lpvaluename.param().abi(),
            lpreserved.unwrap_or(core::mem::zeroed()) as _,
            lptype.unwrap_or(core::mem::zeroed()) as _,
            lpdata.unwrap_or(core::mem::zeroed()) as _,
            lpcbdata as _,
        )
    }
}
#[inline]
pub unsafe fn RegSetValueExW<P1>(
    hkey: HKEY,
    lpvaluename: P1,
    reserved: Option<u32>,
    dwtype: u32,
    lpdata: Option<&[u8]>,
) -> LSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("advapi32.dll" "system" fn RegSetValueExW(hkey : HKEY, lpvaluename : windows_core::PCWSTR, reserved : u32, dwtype : u32, lpdata : *const u8, cbdata : u32) -> LSTATUS);
    unsafe {
        RegSetValueExW(
            hkey,
            lpvaluename.param().abi(),
            reserved.unwrap_or(core::mem::zeroed()) as _,
            dwtype,
            lpdata.map_or(core::ptr::null(), |slice| slice.as_ptr()),
            lpdata.map_or(0, |slice| slice.len().try_into().unwrap()),
        )
    }
}
#[inline]
pub unsafe fn ReleaseMutex(hmutex: HANDLE) -> windows_core::BOOL {
    windows_core::link!("kernel32.dll" "system" fn ReleaseMutex(hmutex : HANDLE) -> windows_core::BOOL);
    unsafe { ReleaseMutex(hmutex) }
}
#[inline]
pub unsafe fn SelectObject(hdc: HDC, h: HGDIOBJ) -> HGDIOBJ {
    windows_core::link!("gdi32.dll" "system" fn SelectObject(hdc : HDC, h : HGDIOBJ) -> HGDIOBJ);
    unsafe { SelectObject(hdc, h) }
}
#[inline]
pub unsafe fn SetBkMode(hdc: HDC, mode: i32) -> i32 {
    windows_core::link!("gdi32.dll" "system" fn SetBkMode(hdc : HDC, mode : i32) -> i32);
    unsafe { SetBkMode(hdc, mode) }
}
#[inline]
pub unsafe fn SetGraphicsMode(hdc: HDC, imode: i32) -> i32 {
    windows_core::link!("gdi32.dll" "system" fn SetGraphicsMode(hdc : HDC, imode : i32) -> i32);
    unsafe { SetGraphicsMode(hdc, imode) }
}
#[inline]
pub unsafe fn SetStretchBltMode(hdc: HDC, mode: i32) -> i32 {
    windows_core::link!("gdi32.dll" "system" fn SetStretchBltMode(hdc : HDC, mode : i32) -> i32);
    unsafe { SetStretchBltMode(hdc, mode) }
}
#[inline]
pub unsafe fn SetTextAlign(hdc: HDC, align: u32) -> u32 {
    windows_core::link!("gdi32.dll" "system" fn SetTextAlign(hdc : HDC, align : u32) -> u32);
    unsafe { SetTextAlign(hdc, align) }
}
#[inline]
pub unsafe fn SetTextColor(hdc: HDC, color: COLORREF) -> COLORREF {
    windows_core::link!("gdi32.dll" "system" fn SetTextColor(hdc : HDC, color : COLORREF) -> COLORREF);
    unsafe { SetTextColor(hdc, color) }
}
#[inline]
pub unsafe fn SetWorldTransform(hdc: HDC, lpxf: *const XFORM) -> windows_core::BOOL {
    windows_core::link!("gdi32.dll" "system" fn SetWorldTransform(hdc : HDC, lpxf : *const XFORM) -> windows_core::BOOL);
    unsafe { SetWorldTransform(hdc, lpxf) }
}
#[inline]
pub unsafe fn StartDocW(hdc: HDC, lpdi: *const DOCINFOW) -> i32 {
    windows_core::link!("gdi32.dll" "system" fn StartDocW(hdc : HDC, lpdi : *const DOCINFOW) -> i32);
    unsafe { StartDocW(hdc, lpdi) }
}
#[inline]
pub unsafe fn StartPage(hdc: HDC) -> i32 {
    windows_core::link!("gdi32.dll" "system" fn StartPage(hdc : HDC) -> i32);
    unsafe { StartPage(hdc) }
}
#[inline]
pub unsafe fn StretchDIBits(
    hdc: HDC,
    xdest: i32,
    ydest: i32,
    destwidth: i32,
    destheight: i32,
    xsrc: i32,
    ysrc: i32,
    srcwidth: i32,
    srcheight: i32,
    lpbits: Option<*const core::ffi::c_void>,
    lpbmi: *const BITMAPINFO,
    iusage: u32,
    rop: u32,
) -> i32 {
    windows_core::link!("gdi32.dll" "system" fn StretchDIBits(hdc : HDC, xdest : i32, ydest : i32, destwidth : i32, destheight : i32, xsrc : i32, ysrc : i32, srcwidth : i32, srcheight : i32, lpbits : *const core::ffi::c_void, lpbmi : *const BITMAPINFO, iusage : u32, rop : u32) -> i32);
    unsafe {
        StretchDIBits(
            hdc,
            xdest,
            ydest,
            destwidth,
            destheight,
            xsrc,
            ysrc,
            srcwidth,
            srcheight,
            lpbits.unwrap_or(core::mem::zeroed()) as _,
            lpbmi,
            iusage,
            rop,
        )
    }
}
#[inline]
pub unsafe fn SysAllocString(psz: Option<*const OLECHAR>) -> windows_core::BSTR {
    windows_core::link!("oleaut32.dll" "system" fn SysAllocString(psz : *const OLECHAR) -> windows_core::BSTR);
    unsafe { SysAllocString(psz.unwrap_or(core::mem::zeroed()) as _) }
}
#[inline]
pub unsafe fn SysFreeString(bstrstring: &windows_core::BSTR) {
    windows_core::link!("oleaut32.dll" "system" fn SysFreeString(bstrstring : *mut core::ffi::c_void));
    unsafe { SysFreeString(core::mem::transmute_copy(bstrstring)) }
}
#[inline]
pub unsafe fn TextOutW(hdc: HDC, x: i32, y: i32, lpstring: &[u16]) -> windows_core::BOOL {
    windows_core::link!("gdi32.dll" "system" fn TextOutW(hdc : HDC, x : i32, y : i32, lpstring : windows_core::PCWSTR, c : i32) -> windows_core::BOOL);
    unsafe {
        TextOutW(
            hdc,
            x,
            y,
            core::mem::transmute(lpstring.as_ptr()),
            lpstring.len().try_into().unwrap(),
        )
    }
}
#[inline]
pub unsafe fn VariantClear(pvarg: *mut VARIANTARG) -> windows_core::HRESULT {
    windows_core::link!("oleaut32.dll" "system" fn VariantClear(pvarg : *mut VARIANTARG) -> windows_core::HRESULT);
    unsafe { VariantClear(pvarg) }
}
#[inline]
pub unsafe fn WaitForSingleObject(hhandle: HANDLE, dwmilliseconds: u32) -> u32 {
    windows_core::link!("kernel32.dll" "system" fn WaitForSingleObject(hhandle : HANDLE, dwmilliseconds : u32) -> u32);
    unsafe { WaitForSingleObject(hhandle, dwmilliseconds) }
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ACCESS_MASK(pub u32);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ADDRESS_FAMILY(pub u16);
pub const AF_INET: i32 = 2;
pub const AF_INET6: i32 = 23;
pub const BCRYPT_AES_ALGORITHM: windows_core::PCWSTR = windows_core::w!("AES");
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BCRYPT_ALG_HANDLE(pub *mut core::ffi::c_void);
pub const BCRYPT_ALG_HANDLE_HMAC_FLAG: i32 = 8;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
    pub cbSize: u32,
    pub dwInfoVersion: u32,
    pub pbNonce: PUCHAR,
    pub cbNonce: u32,
    pub pbAuthData: PUCHAR,
    pub cbAuthData: u32,
    pub pbTag: PUCHAR,
    pub cbTag: u32,
    pub pbMacContext: PUCHAR,
    pub cbMacContext: u32,
    pub cbAAD: u32,
    pub cbData: u64,
    pub dwFlags: u32,
}
pub const BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION: i32 = 1;
pub const BCRYPT_CHAINING_MODE: windows_core::PCWSTR = windows_core::w!("ChainingMode");
pub const BCRYPT_CHAIN_MODE_GCM: windows_core::PCWSTR = windows_core::w!("ChainingModeGCM");
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BCRYPT_HANDLE(pub *mut core::ffi::c_void);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BCRYPT_HASH_HANDLE(pub *mut core::ffi::c_void);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BCRYPT_KEY_HANDLE(pub *mut core::ffi::c_void);
pub const BCRYPT_OBJECT_LENGTH: windows_core::PCWSTR = windows_core::w!("ObjectLength");
pub const BCRYPT_SHA256_ALGORITHM: windows_core::PCWSTR = windows_core::w!("SHA256");
pub const BCRYPT_USE_SYSTEM_PREFERRED_RNG: i32 = 2;
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BITMAPINFO {
    pub bmiHeader: BITMAPINFOHEADER,
    pub bmiColors: [RGBQUAD; 1],
}
impl Default for BITMAPINFO {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BITMAPINFOHEADER {
    pub biSize: u32,
    pub biWidth: i32,
    pub biHeight: i32,
    pub biPlanes: u16,
    pub biBitCount: u16,
    pub biCompression: u32,
    pub biSizeImage: u32,
    pub biXPelsPerMeter: i32,
    pub biYPelsPerMeter: i32,
    pub biClrUsed: u32,
    pub biClrImportant: u32,
}
pub const BI_RGB: i32 = 0;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CIMTYPE(pub i32);
pub const CLEARTYPE_QUALITY: i32 = 5;
pub const CLIP_DEFAULT_PRECIS: i32 = 0;
pub type CLSCTX = u32;
pub const CLSCTX_INPROC_SERVER: CLSCTX = 1;
pub type COINIT = i32;
pub const COINIT_APARTMENTTHREADED: COINIT = 2;
pub const COINIT_MULTITHREADED: COINIT = 0;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct COLORREF(pub u32);
pub type COMPUTER_NAME_FORMAT = i32;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CRYPTPROTECT_PROMPTSTRUCT {
    pub cbSize: u32,
    pub dwPromptFlags: u32,
    pub hwndApp: HWND,
    pub szPrompt: windows_core::PCWSTR,
}
pub const CRYPTPROTECT_UI_FORBIDDEN: i32 = 1;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CRYPT_INTEGER_BLOB {
    pub cbData: u32,
    pub pbData: *mut u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union CY {
    pub Anonymous: CY_0,
    pub int64: i64,
}
impl Default for CY {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CY_0 {
    pub Lo: u32,
    pub Hi: i32,
}
pub const ComputerNameDnsHostname: COMPUTER_NAME_FORMAT = 1;
pub const ComputerNameNetBIOS: COMPUTER_NAME_FORMAT = 0;
pub type DATA_BLOB = CRYPT_INTEGER_BLOB;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DECIMAL {
    pub wReserved: u16,
    pub Anonymous: DECIMAL_0,
    pub Hi32: u32,
    pub Anonymous2: DECIMAL_1,
}
impl Default for DECIMAL {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union DECIMAL_0 {
    pub Anonymous: DECIMAL_0_0,
    pub signscale: u16,
}
impl Default for DECIMAL_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DECIMAL_0_0 {
    pub scale: u8,
    pub sign: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union DECIMAL_1 {
    pub Anonymous: DECIMAL_1_0,
    pub Lo64: u64,
}
impl Default for DECIMAL_1 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DECIMAL_1_0 {
    pub Lo32: u32,
    pub Mid32: u32,
}
pub const DEFAULT_PITCH: i32 = 0;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DEVMODEW {
    pub dmDeviceName: [u16; 32],
    pub dmSpecVersion: u16,
    pub dmDriverVersion: u16,
    pub dmSize: u16,
    pub dmDriverExtra: u16,
    pub dmFields: u32,
    pub Anonymous: DEVMODEW_0,
    pub dmColor: i16,
    pub dmDuplex: i16,
    pub dmYResolution: i16,
    pub dmTTOption: i16,
    pub dmCollate: i16,
    pub dmFormName: [u16; 32],
    pub dmLogPixels: u16,
    pub dmBitsPerPel: u32,
    pub dmPelsWidth: u32,
    pub dmPelsHeight: u32,
    pub Anonymous2: DEVMODEW_1,
    pub dmDisplayFrequency: u32,
    pub dmICMMethod: u32,
    pub dmICMIntent: u32,
    pub dmMediaType: u32,
    pub dmDitherType: u32,
    pub dmReserved1: u32,
    pub dmReserved2: u32,
    pub dmPanningWidth: u32,
    pub dmPanningHeight: u32,
}
impl Default for DEVMODEW {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union DEVMODEW_0 {
    pub Anonymous: DEVMODEW_0_0,
    pub Anonymous2: DEVMODEW_0_1,
}
impl Default for DEVMODEW_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DEVMODEW_0_0 {
    pub dmOrientation: i16,
    pub dmPaperSize: i16,
    pub dmPaperLength: i16,
    pub dmPaperWidth: i16,
    pub dmScale: i16,
    pub dmCopies: i16,
    pub dmDefaultSource: i16,
    pub dmPrintQuality: i16,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DEVMODEW_0_1 {
    pub dmPosition: POINTL,
    pub dmDisplayOrientation: u32,
    pub dmDisplayFixedOutput: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union DEVMODEW_1 {
    pub dmDisplayFlags: u32,
    pub dmNup: u32,
}
impl Default for DEVMODEW_1 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
pub const DIB_RGB_COLORS: i32 = 0;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DOCINFOW {
    pub cbSize: i32,
    pub lpszDocName: windows_core::PCWSTR,
    pub lpszOutput: windows_core::PCWSTR,
    pub lpszDatatype: windows_core::PCWSTR,
    pub fwType: u32,
}
pub const DT_METAFILE: i32 = 5;
pub const EOAC_NONE: EOLE_AUTHENTICATION_CAPABILITIES = 0;
pub type EOLE_AUTHENTICATION_CAPABILITIES = i32;
pub type FARPROC = Option<unsafe extern "system" fn() -> isize>;
pub const FW_BOLD: i32 = 700;
pub const GAA_FLAG_INCLUDE_GATEWAYS: i32 = 128;
pub const GAA_FLAG_SKIP_ANYCAST: i32 = 2;
pub const GAA_FLAG_SKIP_MULTICAST: i32 = 4;
pub const GM_ADVANCED: i32 = 2;
pub const HALFTONE: i32 = 4;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HANDLE(pub *mut core::ffi::c_void);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HDC(pub *mut core::ffi::c_void);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HFONT(pub *mut core::ffi::c_void);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HGDIOBJ(pub *mut core::ffi::c_void);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HINSTANCE(pub *mut core::ffi::c_void);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HKEY(pub *mut core::ffi::c_void);
pub const HKEY_CURRENT_USER: HKEY = HKEY(-2147483647 as _);
pub const HKEY_LOCAL_MACHINE: HKEY = HKEY(-2147483646 as _);
pub type HLOCAL = HANDLE;
pub type HMODULE = HINSTANCE;
pub const HORZRES: i32 = 8;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HWND(pub *mut core::ffi::c_void);
windows_core::imp::define_interface!(
    IDispatch,
    IDispatch_Vtbl,
    0x00020400_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IDispatch, windows_core::IUnknown);
#[repr(C)]
pub struct IDispatch_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    GetTypeInfoCount: usize,
    GetTypeInfo: usize,
    GetIDsOfNames: usize,
    Invoke: usize,
}
impl windows_core::RuntimeName for IDispatch {}
windows_core::imp::define_interface!(
    IEnumWbemClassObject,
    IEnumWbemClassObject_Vtbl,
    0x027947e1_d731_11ce_a357_000000000001
);
windows_core::imp::interface_hierarchy!(IEnumWbemClassObject, windows_core::IUnknown);
impl IEnumWbemClassObject {
    pub unsafe fn Reset(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Reset)(windows_core::Interface::as_raw(self))
        }
    }
    pub unsafe fn Next(
        &self,
        ltimeout: i32,
        ucount: u32,
        apobjects: *mut Option<IWbemClassObject>,
        pureturned: *mut u32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Next)(
                windows_core::Interface::as_raw(self),
                ltimeout,
                ucount,
                core::mem::transmute(apobjects),
                pureturned as _,
            )
        }
    }
    pub unsafe fn NextAsync<P1>(&self, ucount: u32, psink: P1) -> windows_core::HRESULT
    where
        P1: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).NextAsync)(
                windows_core::Interface::as_raw(self),
                ucount,
                psink.param().abi(),
            )
        }
    }
    pub unsafe fn Clone(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Clone)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn Skip(&self, ltimeout: i32, ncount: u32) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Skip)(
                windows_core::Interface::as_raw(self),
                ltimeout,
                ncount,
            )
        }
    }
}
#[repr(C)]
pub struct IEnumWbemClassObject_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub Reset: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub Next: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        i32,
        u32,
        *mut *mut core::ffi::c_void,
        *mut u32,
    ) -> windows_core::HRESULT,
    pub NextAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Clone: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Skip: unsafe extern "system" fn(*mut core::ffi::c_void, i32, u32) -> windows_core::HRESULT,
}
pub trait IEnumWbemClassObject_Impl: windows_core::IUnknownImpl {
    fn Reset(&self) -> windows_core::Result<()>;
    fn Next(
        &self,
        ltimeout: i32,
        ucount: u32,
        apobjects: windows_core::OutRef<IWbemClassObject>,
        pureturned: *mut u32,
    ) -> windows_core::Result<()>;
    fn NextAsync(
        &self,
        ucount: u32,
        psink: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn Clone(&self) -> windows_core::Result<IEnumWbemClassObject>;
    fn Skip(&self, ltimeout: i32, ncount: u32) -> windows_core::Result<()>;
}
impl IEnumWbemClassObject_Vtbl {
    pub const fn new<Identity: IEnumWbemClassObject_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn Reset<
            Identity: IEnumWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IEnumWbemClassObject_Impl::Reset(this).into()
            }
        }
        unsafe extern "system" fn Next<Identity: IEnumWbemClassObject_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            ltimeout: i32,
            ucount: u32,
            apobjects: *mut *mut core::ffi::c_void,
            pureturned: *mut u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IEnumWbemClassObject_Impl::Next(
                    this,
                    core::mem::transmute_copy(&ltimeout),
                    core::mem::transmute_copy(&ucount),
                    core::mem::transmute_copy(&apobjects),
                    core::mem::transmute_copy(&pureturned),
                )
                .into()
            }
        }
        unsafe extern "system" fn NextAsync<
            Identity: IEnumWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            ucount: u32,
            psink: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IEnumWbemClassObject_Impl::NextAsync(
                    this,
                    core::mem::transmute_copy(&ucount),
                    core::mem::transmute_copy(&psink),
                )
                .into()
            }
        }
        unsafe extern "system" fn Clone<
            Identity: IEnumWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            ppenum: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IEnumWbemClassObject_Impl::Clone(this) {
                    Ok(ok__) => {
                        ppenum.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn Skip<Identity: IEnumWbemClassObject_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            ltimeout: i32,
            ncount: u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IEnumWbemClassObject_Impl::Skip(
                    this,
                    core::mem::transmute_copy(&ltimeout),
                    core::mem::transmute_copy(&ncount),
                )
                .into()
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            Reset: Reset::<Identity, OFFSET>,
            Next: Next::<Identity, OFFSET>,
            NextAsync: NextAsync::<Identity, OFFSET>,
            Clone: Clone::<Identity, OFFSET>,
            Skip: Skip::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IEnumWbemClassObject as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IEnumWbemClassObject {}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IFTYPE(pub u32);
pub type IF_INDEX = NET_IFINDEX;
pub type IF_LUID = NET_LUID;
pub type IF_OPER_STATUS = i32;
pub const IF_TYPE_SOFTWARE_LOOPBACK: i32 = 24;
pub const IF_TYPE_TUNNEL: i32 = 131;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IN6_ADDR {
    pub u: IN6_ADDR_0,
}
impl Default for IN6_ADDR {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IN6_ADDR_0 {
    pub Byte: [u8; 16],
    pub Word: [u16; 8],
}
impl Default for IN6_ADDR_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IN_ADDR {
    pub S_un: IN_ADDR_0,
}
impl Default for IN_ADDR {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IN_ADDR_0 {
    pub S_un_b: IN_ADDR_0_0,
    pub S_un_w: IN_ADDR_0_1,
    pub S_addr: u32,
}
impl Default for IN_ADDR_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IN_ADDR_0_0 {
    pub s_b1: u8,
    pub s_b2: u8,
    pub s_b3: u8,
    pub s_b4: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IN_ADDR_0_1 {
    pub s_w1: u16,
    pub s_w2: u16,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IP_ADAPTER_ADDRESSES_LH {
    pub Anonymous: IP_ADAPTER_ADDRESSES_LH_0,
    pub Next: *mut Self,
    pub AdapterName: PCHAR,
    pub FirstUnicastAddress: PIP_ADAPTER_UNICAST_ADDRESS_LH,
    pub FirstAnycastAddress: PIP_ADAPTER_ANYCAST_ADDRESS_XP,
    pub FirstMulticastAddress: PIP_ADAPTER_MULTICAST_ADDRESS_XP,
    pub FirstDnsServerAddress: PIP_ADAPTER_DNS_SERVER_ADDRESS_XP,
    pub DnsSuffix: PWCHAR,
    pub Description: PWCHAR,
    pub FriendlyName: PWCHAR,
    pub PhysicalAddress: [u8; 8],
    pub PhysicalAddressLength: u32,
    pub Anonymous2: IP_ADAPTER_ADDRESSES_LH_1,
    pub Mtu: u32,
    pub IfType: IFTYPE,
    pub OperStatus: IF_OPER_STATUS,
    pub Ipv6IfIndex: IF_INDEX,
    pub ZoneIndices: [u32; 16],
    pub FirstPrefix: PIP_ADAPTER_PREFIX_XP,
    pub TransmitLinkSpeed: u64,
    pub ReceiveLinkSpeed: u64,
    pub FirstWinsServerAddress: PIP_ADAPTER_WINS_SERVER_ADDRESS_LH,
    pub FirstGatewayAddress: PIP_ADAPTER_GATEWAY_ADDRESS_LH,
    pub Ipv4Metric: u32,
    pub Ipv6Metric: u32,
    pub Luid: IF_LUID,
    pub Dhcpv4Server: SOCKET_ADDRESS,
    pub CompartmentId: NET_IF_COMPARTMENT_ID,
    pub NetworkGuid: NET_IF_NETWORK_GUID,
    pub ConnectionType: NET_IF_CONNECTION_TYPE,
    pub TunnelType: TUNNEL_TYPE,
    pub Dhcpv6Server: SOCKET_ADDRESS,
    pub Dhcpv6ClientDuid: [u8; 130],
    pub Dhcpv6ClientDuidLength: u32,
    pub Dhcpv6Iaid: u32,
    pub FirstDnsSuffix: PIP_ADAPTER_DNS_SUFFIX,
}
impl Default for IP_ADAPTER_ADDRESSES_LH {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IP_ADAPTER_ADDRESSES_LH_0 {
    pub Alignment: u64,
    pub Anonymous: IP_ADAPTER_ADDRESSES_LH_0_0,
}
impl Default for IP_ADAPTER_ADDRESSES_LH_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IP_ADAPTER_ADDRESSES_LH_0_0 {
    pub Length: u32,
    pub IfIndex: IF_INDEX,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IP_ADAPTER_ADDRESSES_LH_1 {
    pub Flags: u32,
    pub Anonymous: IP_ADAPTER_ADDRESSES_LH_1_0,
}
impl Default for IP_ADAPTER_ADDRESSES_LH_1 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IP_ADAPTER_ADDRESSES_LH_1_0 {
    pub _bitfield: u32,
}
impl IP_ADAPTER_ADDRESSES_LH_1_0 {
    pub fn DdnsEnabled(&self) -> bool {
        self._bitfield & 1 != 0
    }
    pub fn set_DdnsEnabled(&mut self, value: bool) {
        self._bitfield = (self._bitfield & !1) | (value as u32);
    }
    pub fn RegisterAdapterSuffix(&self) -> bool {
        (self._bitfield >> 1) & 1 != 0
    }
    pub fn set_RegisterAdapterSuffix(&mut self, value: bool) {
        self._bitfield = (self._bitfield & !(1 << 1)) | ((value as u32) << 1);
    }
    pub fn Dhcpv4Enabled(&self) -> bool {
        (self._bitfield >> 2) & 1 != 0
    }
    pub fn set_Dhcpv4Enabled(&mut self, value: bool) {
        self._bitfield = (self._bitfield & !(1 << 2)) | ((value as u32) << 2);
    }
    pub fn ReceiveOnly(&self) -> bool {
        (self._bitfield >> 3) & 1 != 0
    }
    pub fn set_ReceiveOnly(&mut self, value: bool) {
        self._bitfield = (self._bitfield & !(1 << 3)) | ((value as u32) << 3);
    }
    pub fn NoMulticast(&self) -> bool {
        (self._bitfield >> 4) & 1 != 0
    }
    pub fn set_NoMulticast(&mut self, value: bool) {
        self._bitfield = (self._bitfield & !(1 << 4)) | ((value as u32) << 4);
    }
    pub fn Ipv6OtherStatefulConfig(&self) -> bool {
        (self._bitfield >> 5) & 1 != 0
    }
    pub fn set_Ipv6OtherStatefulConfig(&mut self, value: bool) {
        self._bitfield = (self._bitfield & !(1 << 5)) | ((value as u32) << 5);
    }
    pub fn NetbiosOverTcpipEnabled(&self) -> bool {
        (self._bitfield >> 6) & 1 != 0
    }
    pub fn set_NetbiosOverTcpipEnabled(&mut self, value: bool) {
        self._bitfield = (self._bitfield & !(1 << 6)) | ((value as u32) << 6);
    }
    pub fn Ipv4Enabled(&self) -> bool {
        (self._bitfield >> 7) & 1 != 0
    }
    pub fn set_Ipv4Enabled(&mut self, value: bool) {
        self._bitfield = (self._bitfield & !(1 << 7)) | ((value as u32) << 7);
    }
    pub fn Ipv6Enabled(&self) -> bool {
        (self._bitfield >> 8) & 1 != 0
    }
    pub fn set_Ipv6Enabled(&mut self, value: bool) {
        self._bitfield = (self._bitfield & !(1 << 8)) | ((value as u32) << 8);
    }
    pub fn Ipv6ManagedAddressConfigurationSupported(&self) -> bool {
        (self._bitfield >> 9) & 1 != 0
    }
    pub fn set_Ipv6ManagedAddressConfigurationSupported(&mut self, value: bool) {
        self._bitfield = (self._bitfield & !(1 << 9)) | ((value as u32) << 9);
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IP_ADAPTER_ANYCAST_ADDRESS_XP {
    pub Anonymous: IP_ADAPTER_ANYCAST_ADDRESS_XP_0,
    pub Next: *mut Self,
    pub Address: SOCKET_ADDRESS,
}
impl Default for IP_ADAPTER_ANYCAST_ADDRESS_XP {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IP_ADAPTER_ANYCAST_ADDRESS_XP_0 {
    pub Alignment: u64,
    pub Anonymous: IP_ADAPTER_ANYCAST_ADDRESS_XP_0_0,
}
impl Default for IP_ADAPTER_ANYCAST_ADDRESS_XP_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IP_ADAPTER_ANYCAST_ADDRESS_XP_0_0 {
    pub Length: u32,
    pub Flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IP_ADAPTER_DNS_SERVER_ADDRESS_XP {
    pub Anonymous: IP_ADAPTER_DNS_SERVER_ADDRESS_XP_0,
    pub Next: *mut Self,
    pub Address: SOCKET_ADDRESS,
}
impl Default for IP_ADAPTER_DNS_SERVER_ADDRESS_XP {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IP_ADAPTER_DNS_SERVER_ADDRESS_XP_0 {
    pub Alignment: u64,
    pub Anonymous: IP_ADAPTER_DNS_SERVER_ADDRESS_XP_0_0,
}
impl Default for IP_ADAPTER_DNS_SERVER_ADDRESS_XP_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IP_ADAPTER_DNS_SERVER_ADDRESS_XP_0_0 {
    pub Length: u32,
    pub Reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IP_ADAPTER_DNS_SUFFIX {
    pub Next: *mut Self,
    pub String: [u16; 256],
}
impl Default for IP_ADAPTER_DNS_SUFFIX {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IP_ADAPTER_GATEWAY_ADDRESS_LH {
    pub Anonymous: IP_ADAPTER_GATEWAY_ADDRESS_LH_0,
    pub Next: *mut Self,
    pub Address: SOCKET_ADDRESS,
}
impl Default for IP_ADAPTER_GATEWAY_ADDRESS_LH {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IP_ADAPTER_GATEWAY_ADDRESS_LH_0 {
    pub Alignment: u64,
    pub Anonymous: IP_ADAPTER_GATEWAY_ADDRESS_LH_0_0,
}
impl Default for IP_ADAPTER_GATEWAY_ADDRESS_LH_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IP_ADAPTER_GATEWAY_ADDRESS_LH_0_0 {
    pub Length: u32,
    pub Reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IP_ADAPTER_MULTICAST_ADDRESS_XP {
    pub Anonymous: IP_ADAPTER_MULTICAST_ADDRESS_XP_0,
    pub Next: *mut Self,
    pub Address: SOCKET_ADDRESS,
}
impl Default for IP_ADAPTER_MULTICAST_ADDRESS_XP {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IP_ADAPTER_MULTICAST_ADDRESS_XP_0 {
    pub Alignment: u64,
    pub Anonymous: IP_ADAPTER_MULTICAST_ADDRESS_XP_0_0,
}
impl Default for IP_ADAPTER_MULTICAST_ADDRESS_XP_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IP_ADAPTER_MULTICAST_ADDRESS_XP_0_0 {
    pub Length: u32,
    pub Flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IP_ADAPTER_PREFIX_XP {
    pub Anonymous: IP_ADAPTER_PREFIX_XP_0,
    pub Next: *mut Self,
    pub Address: SOCKET_ADDRESS,
    pub PrefixLength: u32,
}
impl Default for IP_ADAPTER_PREFIX_XP {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IP_ADAPTER_PREFIX_XP_0 {
    pub Alignment: u64,
    pub Anonymous: IP_ADAPTER_PREFIX_XP_0_0,
}
impl Default for IP_ADAPTER_PREFIX_XP_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IP_ADAPTER_PREFIX_XP_0_0 {
    pub Length: u32,
    pub Flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IP_ADAPTER_UNICAST_ADDRESS_LH {
    pub Anonymous: IP_ADAPTER_UNICAST_ADDRESS_LH_0,
    pub Next: *mut Self,
    pub Address: SOCKET_ADDRESS,
    pub PrefixOrigin: IP_PREFIX_ORIGIN,
    pub SuffixOrigin: IP_SUFFIX_ORIGIN,
    pub DadState: IP_DAD_STATE,
    pub ValidLifetime: u32,
    pub PreferredLifetime: u32,
    pub LeaseLifetime: u32,
    pub OnLinkPrefixLength: u8,
}
impl Default for IP_ADAPTER_UNICAST_ADDRESS_LH {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IP_ADAPTER_UNICAST_ADDRESS_LH_0 {
    pub Alignment: u64,
    pub Anonymous: IP_ADAPTER_UNICAST_ADDRESS_LH_0_0,
}
impl Default for IP_ADAPTER_UNICAST_ADDRESS_LH_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IP_ADAPTER_UNICAST_ADDRESS_LH_0_0 {
    pub Length: u32,
    pub Flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IP_ADAPTER_WINS_SERVER_ADDRESS_LH {
    pub Anonymous: IP_ADAPTER_WINS_SERVER_ADDRESS_LH_0,
    pub Next: *mut Self,
    pub Address: SOCKET_ADDRESS,
}
impl Default for IP_ADAPTER_WINS_SERVER_ADDRESS_LH {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union IP_ADAPTER_WINS_SERVER_ADDRESS_LH_0 {
    pub Alignment: u64,
    pub Anonymous: IP_ADAPTER_WINS_SERVER_ADDRESS_LH_0_0,
}
impl Default for IP_ADAPTER_WINS_SERVER_ADDRESS_LH_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IP_ADAPTER_WINS_SERVER_ADDRESS_LH_0_0 {
    pub Length: u32,
    pub Reserved: u32,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IP_DAD_STATE(pub NL_DAD_STATE);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IP_PREFIX_ORIGIN(pub NL_PREFIX_ORIGIN);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IP_SUFFIX_ORIGIN(pub NL_SUFFIX_ORIGIN);
windows_core::imp::define_interface!(
    IRecordInfo,
    IRecordInfo_Vtbl,
    0x0000002f_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IRecordInfo, windows_core::IUnknown);
#[repr(C)]
pub struct IRecordInfo_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    RecordInit: usize,
    RecordClear: usize,
    RecordCopy: usize,
    GetGuid: usize,
    GetName: usize,
    GetSize: usize,
    GetTypeInfo: usize,
    GetField: usize,
    GetFieldNoCopy: usize,
    PutField: usize,
    PutFieldNoCopy: usize,
    GetFieldNames: usize,
    IsMatchingType: usize,
    RecordCreate: usize,
    RecordCreateCopy: usize,
    RecordDestroy: usize,
}
impl windows_core::RuntimeName for IRecordInfo {}
windows_core::imp::define_interface!(
    IWbemCallResult,
    IWbemCallResult_Vtbl,
    0x44aca675_e8fc_11d0_a07c_00c04fb68820
);
windows_core::imp::interface_hierarchy!(IWbemCallResult, windows_core::IUnknown);
#[repr(C)]
pub struct IWbemCallResult_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    GetResultObject: usize,
    GetResultString: usize,
    GetResultServices: usize,
    GetCallStatus: usize,
}
impl windows_core::RuntimeName for IWbemCallResult {}
windows_core::imp::define_interface!(
    IWbemClassObject,
    IWbemClassObject_Vtbl,
    0xdc12a681_737f_11cf_884d_00aa004b2e24
);
windows_core::imp::interface_hierarchy!(IWbemClassObject, windows_core::IUnknown);
impl IWbemClassObject {
    pub unsafe fn GetQualifierSet(&self) -> windows_core::Result<IWbemQualifierSet> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetQualifierSet)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn Get<P0>(
        &self,
        wszname: P0,
        lflags: i32,
        pval: *mut VARIANT,
        ptype: *mut CIMTYPE,
        plflavor: *mut i32,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).Get)(
                windows_core::Interface::as_raw(self),
                wszname.param().abi(),
                lflags,
                pval,
                ptype as _,
                plflavor as _,
            )
        }
    }
    pub unsafe fn Put<P0>(
        &self,
        wszname: P0,
        lflags: i32,
        pval: *const VARIANT,
        r#type: CIMTYPE,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).Put)(
                windows_core::Interface::as_raw(self),
                wszname.param().abi(),
                lflags,
                pval,
                r#type,
            )
        }
    }
    pub unsafe fn Delete<P0>(&self, wszname: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).Delete)(
                windows_core::Interface::as_raw(self),
                wszname.param().abi(),
            )
        }
    }
    pub unsafe fn GetNames<P0>(
        &self,
        wszqualifiername: P0,
        lflags: i32,
        pqualifierval: *const VARIANT,
    ) -> windows_core::Result<*mut SAFEARRAY>
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetNames)(
                windows_core::Interface::as_raw(self),
                wszqualifiername.param().abi(),
                lflags,
                pqualifierval,
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn BeginEnumeration(&self, lenumflags: i32) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).BeginEnumeration)(
                windows_core::Interface::as_raw(self),
                lenumflags,
            )
        }
    }
    pub unsafe fn Next(
        &self,
        lflags: i32,
        strname: *mut windows_core::BSTR,
        pval: *mut VARIANT,
        ptype: *mut CIMTYPE,
        plflavor: *mut i32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Next)(
                windows_core::Interface::as_raw(self),
                lflags,
                core::mem::transmute(strname),
                pval,
                ptype as _,
                plflavor as _,
            )
        }
    }
    pub unsafe fn EndEnumeration(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).EndEnumeration)(windows_core::Interface::as_raw(
                self,
            ))
        }
    }
    pub unsafe fn GetPropertyQualifierSet<P0>(
        &self,
        wszproperty: P0,
    ) -> windows_core::Result<IWbemQualifierSet>
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetPropertyQualifierSet)(
                windows_core::Interface::as_raw(self),
                wszproperty.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn Clone(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Clone)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetObjectText(&self, lflags: i32) -> windows_core::Result<windows_core::BSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetObjectText)(
                windows_core::Interface::as_raw(self),
                lflags,
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn SpawnDerivedClass(&self, lflags: i32) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).SpawnDerivedClass)(
                windows_core::Interface::as_raw(self),
                lflags,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn SpawnInstance(&self, lflags: i32) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).SpawnInstance)(
                windows_core::Interface::as_raw(self),
                lflags,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CompareTo<P1>(&self, lflags: i32, pcompareto: P1) -> windows_core::HRESULT
    where
        P1: windows_core::Param<Self>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).CompareTo)(
                windows_core::Interface::as_raw(self),
                lflags,
                pcompareto.param().abi(),
            )
        }
    }
    pub unsafe fn GetPropertyOrigin<P0>(
        &self,
        wszname: P0,
    ) -> windows_core::Result<windows_core::BSTR>
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetPropertyOrigin)(
                windows_core::Interface::as_raw(self),
                wszname.param().abi(),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
    pub unsafe fn InheritsFrom<P0>(&self, strancestor: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).InheritsFrom)(
                windows_core::Interface::as_raw(self),
                strancestor.param().abi(),
            )
        }
    }
    pub unsafe fn GetMethod<P0>(
        &self,
        wszname: P0,
        lflags: i32,
        ppinsignature: *mut Option<Self>,
        ppoutsignature: *mut Option<Self>,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).GetMethod)(
                windows_core::Interface::as_raw(self),
                wszname.param().abi(),
                lflags,
                core::mem::transmute(ppinsignature),
                core::mem::transmute(ppoutsignature),
            )
        }
    }
    pub unsafe fn PutMethod<P0, P2, P3>(
        &self,
        wszname: P0,
        lflags: i32,
        pinsignature: P2,
        poutsignature: P3,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
        P2: windows_core::Param<Self>,
        P3: windows_core::Param<Self>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).PutMethod)(
                windows_core::Interface::as_raw(self),
                wszname.param().abi(),
                lflags,
                pinsignature.param().abi(),
                poutsignature.param().abi(),
            )
        }
    }
    pub unsafe fn DeleteMethod<P0>(&self, wszname: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).DeleteMethod)(
                windows_core::Interface::as_raw(self),
                wszname.param().abi(),
            )
        }
    }
    pub unsafe fn BeginMethodEnumeration(&self, lenumflags: i32) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).BeginMethodEnumeration)(
                windows_core::Interface::as_raw(self),
                lenumflags,
            )
        }
    }
    pub unsafe fn NextMethod(
        &self,
        lflags: i32,
        pstrname: *mut windows_core::BSTR,
        ppinsignature: *mut Option<Self>,
        ppoutsignature: *mut Option<Self>,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).NextMethod)(
                windows_core::Interface::as_raw(self),
                lflags,
                core::mem::transmute(pstrname),
                core::mem::transmute(ppinsignature),
                core::mem::transmute(ppoutsignature),
            )
        }
    }
    pub unsafe fn EndMethodEnumeration(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).EndMethodEnumeration)(
                windows_core::Interface::as_raw(self),
            )
        }
    }
    pub unsafe fn GetMethodQualifierSet<P0>(
        &self,
        wszmethod: P0,
    ) -> windows_core::Result<IWbemQualifierSet>
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetMethodQualifierSet)(
                windows_core::Interface::as_raw(self),
                wszmethod.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetMethodOrigin<P0>(
        &self,
        wszmethodname: P0,
    ) -> windows_core::Result<windows_core::BSTR>
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetMethodOrigin)(
                windows_core::Interface::as_raw(self),
                wszmethodname.param().abi(),
                &mut result__,
            )
            .map(|| core::mem::transmute(result__))
        }
    }
}
#[repr(C)]
pub struct IWbemClassObject_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub GetQualifierSet: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Get: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        i32,
        *mut VARIANT,
        *mut CIMTYPE,
        *mut i32,
    ) -> windows_core::HRESULT,
    pub Put: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        i32,
        *const VARIANT,
        CIMTYPE,
    ) -> windows_core::HRESULT,
    pub Delete: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
    pub GetNames: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        i32,
        *const VARIANT,
        *mut *mut SAFEARRAY,
    ) -> windows_core::HRESULT,
    pub BeginEnumeration:
        unsafe extern "system" fn(*mut core::ffi::c_void, i32) -> windows_core::HRESULT,
    pub Next: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        i32,
        *mut *mut core::ffi::c_void,
        *mut VARIANT,
        *mut CIMTYPE,
        *mut i32,
    ) -> windows_core::HRESULT,
    pub EndEnumeration: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub GetPropertyQualifierSet: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Clone: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetObjectText: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        i32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SpawnDerivedClass: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        i32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SpawnInstance: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        i32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CompareTo: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetPropertyOrigin: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub InheritsFrom: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
    pub GetMethod: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        i32,
        *mut *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub PutMethod: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub DeleteMethod: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
    pub BeginMethodEnumeration:
        unsafe extern "system" fn(*mut core::ffi::c_void, i32) -> windows_core::HRESULT,
    pub NextMethod: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        i32,
        *mut *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub EndMethodEnumeration:
        unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub GetMethodQualifierSet: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetMethodOrigin: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
pub trait IWbemClassObject_Impl: windows_core::IUnknownImpl {
    fn GetQualifierSet(&self) -> windows_core::Result<IWbemQualifierSet>;
    fn Get(
        &self,
        wszname: &windows_core::PCWSTR,
        lflags: i32,
        pval: *mut VARIANT,
        ptype: *mut CIMTYPE,
        plflavor: *mut i32,
    ) -> windows_core::Result<()>;
    fn Put(
        &self,
        wszname: &windows_core::PCWSTR,
        lflags: i32,
        pval: *const VARIANT,
        r#type: CIMTYPE,
    ) -> windows_core::Result<()>;
    fn Delete(&self, wszname: &windows_core::PCWSTR) -> windows_core::Result<()>;
    fn GetNames(
        &self,
        wszqualifiername: &windows_core::PCWSTR,
        lflags: i32,
        pqualifierval: *const VARIANT,
    ) -> windows_core::Result<*mut SAFEARRAY>;
    fn BeginEnumeration(&self, lenumflags: i32) -> windows_core::Result<()>;
    fn Next(
        &self,
        lflags: i32,
        strname: *mut windows_core::BSTR,
        pval: *mut VARIANT,
        ptype: *mut CIMTYPE,
        plflavor: *mut i32,
    ) -> windows_core::Result<()>;
    fn EndEnumeration(&self) -> windows_core::Result<()>;
    fn GetPropertyQualifierSet(
        &self,
        wszproperty: &windows_core::PCWSTR,
    ) -> windows_core::Result<IWbemQualifierSet>;
    fn Clone(&self) -> windows_core::Result<IWbemClassObject>;
    fn GetObjectText(&self, lflags: i32) -> windows_core::Result<windows_core::BSTR>;
    fn SpawnDerivedClass(&self, lflags: i32) -> windows_core::Result<IWbemClassObject>;
    fn SpawnInstance(&self, lflags: i32) -> windows_core::Result<IWbemClassObject>;
    fn CompareTo(
        &self,
        lflags: i32,
        pcompareto: windows_core::Ref<IWbemClassObject>,
    ) -> windows_core::Result<()>;
    fn GetPropertyOrigin(
        &self,
        wszname: &windows_core::PCWSTR,
    ) -> windows_core::Result<windows_core::BSTR>;
    fn InheritsFrom(&self, strancestor: &windows_core::PCWSTR) -> windows_core::Result<()>;
    fn GetMethod(
        &self,
        wszname: &windows_core::PCWSTR,
        lflags: i32,
        ppinsignature: windows_core::OutRef<IWbemClassObject>,
        ppoutsignature: windows_core::OutRef<IWbemClassObject>,
    ) -> windows_core::Result<()>;
    fn PutMethod(
        &self,
        wszname: &windows_core::PCWSTR,
        lflags: i32,
        pinsignature: windows_core::Ref<IWbemClassObject>,
        poutsignature: windows_core::Ref<IWbemClassObject>,
    ) -> windows_core::Result<()>;
    fn DeleteMethod(&self, wszname: &windows_core::PCWSTR) -> windows_core::Result<()>;
    fn BeginMethodEnumeration(&self, lenumflags: i32) -> windows_core::Result<()>;
    fn NextMethod(
        &self,
        lflags: i32,
        pstrname: *mut windows_core::BSTR,
        ppinsignature: windows_core::OutRef<IWbemClassObject>,
        ppoutsignature: windows_core::OutRef<IWbemClassObject>,
    ) -> windows_core::Result<()>;
    fn EndMethodEnumeration(&self) -> windows_core::Result<()>;
    fn GetMethodQualifierSet(
        &self,
        wszmethod: &windows_core::PCWSTR,
    ) -> windows_core::Result<IWbemQualifierSet>;
    fn GetMethodOrigin(
        &self,
        wszmethodname: &windows_core::PCWSTR,
    ) -> windows_core::Result<windows_core::BSTR>;
}
impl IWbemClassObject_Vtbl {
    pub const fn new<Identity: IWbemClassObject_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn GetQualifierSet<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            ppqualset: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemClassObject_Impl::GetQualifierSet(this) {
                    Ok(ok__) => {
                        ppqualset.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn Get<Identity: IWbemClassObject_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            wszname: windows_core::PCWSTR,
            lflags: i32,
            pval: *mut VARIANT,
            ptype: *mut CIMTYPE,
            plflavor: *mut i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::Get(
                    this,
                    core::mem::transmute(&wszname),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pval),
                    core::mem::transmute_copy(&ptype),
                    core::mem::transmute_copy(&plflavor),
                )
                .into()
            }
        }
        unsafe extern "system" fn Put<Identity: IWbemClassObject_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            wszname: windows_core::PCWSTR,
            lflags: i32,
            pval: *const VARIANT,
            r#type: CIMTYPE,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::Put(
                    this,
                    core::mem::transmute(&wszname),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pval),
                    core::mem::transmute_copy(&r#type),
                )
                .into()
            }
        }
        unsafe extern "system" fn Delete<Identity: IWbemClassObject_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            wszname: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::Delete(this, core::mem::transmute(&wszname)).into()
            }
        }
        unsafe extern "system" fn GetNames<Identity: IWbemClassObject_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            wszqualifiername: windows_core::PCWSTR,
            lflags: i32,
            pqualifierval: *const VARIANT,
            pnames: *mut *mut SAFEARRAY,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemClassObject_Impl::GetNames(
                    this,
                    core::mem::transmute(&wszqualifiername),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pqualifierval),
                ) {
                    Ok(ok__) => {
                        pnames.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn BeginEnumeration<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            lenumflags: i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::BeginEnumeration(
                    this,
                    core::mem::transmute_copy(&lenumflags),
                )
                .into()
            }
        }
        unsafe extern "system" fn Next<Identity: IWbemClassObject_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            lflags: i32,
            strname: *mut *mut core::ffi::c_void,
            pval: *mut VARIANT,
            ptype: *mut CIMTYPE,
            plflavor: *mut i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::Next(
                    this,
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&strname),
                    core::mem::transmute_copy(&pval),
                    core::mem::transmute_copy(&ptype),
                    core::mem::transmute_copy(&plflavor),
                )
                .into()
            }
        }
        unsafe extern "system" fn EndEnumeration<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::EndEnumeration(this).into()
            }
        }
        unsafe extern "system" fn GetPropertyQualifierSet<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            wszproperty: windows_core::PCWSTR,
            ppqualset: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemClassObject_Impl::GetPropertyQualifierSet(
                    this,
                    core::mem::transmute(&wszproperty),
                ) {
                    Ok(ok__) => {
                        ppqualset.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn Clone<Identity: IWbemClassObject_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            ppcopy: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemClassObject_Impl::Clone(this) {
                    Ok(ok__) => {
                        ppcopy.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn GetObjectText<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            lflags: i32,
            pstrobjecttext: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemClassObject_Impl::GetObjectText(this, core::mem::transmute_copy(&lflags))
                {
                    Ok(ok__) => {
                        pstrobjecttext.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn SpawnDerivedClass<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            lflags: i32,
            ppnewclass: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemClassObject_Impl::SpawnDerivedClass(
                    this,
                    core::mem::transmute_copy(&lflags),
                ) {
                    Ok(ok__) => {
                        ppnewclass.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn SpawnInstance<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            lflags: i32,
            ppnewinstance: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemClassObject_Impl::SpawnInstance(this, core::mem::transmute_copy(&lflags))
                {
                    Ok(ok__) => {
                        ppnewinstance.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn CompareTo<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            lflags: i32,
            pcompareto: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::CompareTo(
                    this,
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pcompareto),
                )
                .into()
            }
        }
        unsafe extern "system" fn GetPropertyOrigin<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            wszname: windows_core::PCWSTR,
            pstrclassname: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemClassObject_Impl::GetPropertyOrigin(this, core::mem::transmute(&wszname))
                {
                    Ok(ok__) => {
                        pstrclassname.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn InheritsFrom<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strancestor: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::InheritsFrom(this, core::mem::transmute(&strancestor)).into()
            }
        }
        unsafe extern "system" fn GetMethod<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            wszname: windows_core::PCWSTR,
            lflags: i32,
            ppinsignature: *mut *mut core::ffi::c_void,
            ppoutsignature: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::GetMethod(
                    this,
                    core::mem::transmute(&wszname),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&ppinsignature),
                    core::mem::transmute_copy(&ppoutsignature),
                )
                .into()
            }
        }
        unsafe extern "system" fn PutMethod<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            wszname: windows_core::PCWSTR,
            lflags: i32,
            pinsignature: *mut core::ffi::c_void,
            poutsignature: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::PutMethod(
                    this,
                    core::mem::transmute(&wszname),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pinsignature),
                    core::mem::transmute_copy(&poutsignature),
                )
                .into()
            }
        }
        unsafe extern "system" fn DeleteMethod<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            wszname: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::DeleteMethod(this, core::mem::transmute(&wszname)).into()
            }
        }
        unsafe extern "system" fn BeginMethodEnumeration<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            lenumflags: i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::BeginMethodEnumeration(
                    this,
                    core::mem::transmute_copy(&lenumflags),
                )
                .into()
            }
        }
        unsafe extern "system" fn NextMethod<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            lflags: i32,
            pstrname: *mut *mut core::ffi::c_void,
            ppinsignature: *mut *mut core::ffi::c_void,
            ppoutsignature: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::NextMethod(
                    this,
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pstrname),
                    core::mem::transmute_copy(&ppinsignature),
                    core::mem::transmute_copy(&ppoutsignature),
                )
                .into()
            }
        }
        unsafe extern "system" fn EndMethodEnumeration<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemClassObject_Impl::EndMethodEnumeration(this).into()
            }
        }
        unsafe extern "system" fn GetMethodQualifierSet<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            wszmethod: windows_core::PCWSTR,
            ppqualset: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemClassObject_Impl::GetMethodQualifierSet(
                    this,
                    core::mem::transmute(&wszmethod),
                ) {
                    Ok(ok__) => {
                        ppqualset.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn GetMethodOrigin<
            Identity: IWbemClassObject_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            wszmethodname: windows_core::PCWSTR,
            pstrclassname: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemClassObject_Impl::GetMethodOrigin(
                    this,
                    core::mem::transmute(&wszmethodname),
                ) {
                    Ok(ok__) => {
                        pstrclassname.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            GetQualifierSet: GetQualifierSet::<Identity, OFFSET>,
            Get: Get::<Identity, OFFSET>,
            Put: Put::<Identity, OFFSET>,
            Delete: Delete::<Identity, OFFSET>,
            GetNames: GetNames::<Identity, OFFSET>,
            BeginEnumeration: BeginEnumeration::<Identity, OFFSET>,
            Next: Next::<Identity, OFFSET>,
            EndEnumeration: EndEnumeration::<Identity, OFFSET>,
            GetPropertyQualifierSet: GetPropertyQualifierSet::<Identity, OFFSET>,
            Clone: Clone::<Identity, OFFSET>,
            GetObjectText: GetObjectText::<Identity, OFFSET>,
            SpawnDerivedClass: SpawnDerivedClass::<Identity, OFFSET>,
            SpawnInstance: SpawnInstance::<Identity, OFFSET>,
            CompareTo: CompareTo::<Identity, OFFSET>,
            GetPropertyOrigin: GetPropertyOrigin::<Identity, OFFSET>,
            InheritsFrom: InheritsFrom::<Identity, OFFSET>,
            GetMethod: GetMethod::<Identity, OFFSET>,
            PutMethod: PutMethod::<Identity, OFFSET>,
            DeleteMethod: DeleteMethod::<Identity, OFFSET>,
            BeginMethodEnumeration: BeginMethodEnumeration::<Identity, OFFSET>,
            NextMethod: NextMethod::<Identity, OFFSET>,
            EndMethodEnumeration: EndMethodEnumeration::<Identity, OFFSET>,
            GetMethodQualifierSet: GetMethodQualifierSet::<Identity, OFFSET>,
            GetMethodOrigin: GetMethodOrigin::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IWbemClassObject as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IWbemClassObject {}
windows_core::imp::define_interface!(
    IWbemContext,
    IWbemContext_Vtbl,
    0x44aca674_e8fc_11d0_a07c_00c04fb68820
);
windows_core::imp::interface_hierarchy!(IWbemContext, windows_core::IUnknown);
#[repr(C)]
pub struct IWbemContext_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    Clone: usize,
    GetNames: usize,
    BeginEnumeration: usize,
    Next: usize,
    EndEnumeration: usize,
    SetValue: usize,
    GetValue: usize,
    DeleteValue: usize,
    DeleteAll: usize,
}
impl windows_core::RuntimeName for IWbemContext {}
windows_core::imp::define_interface!(
    IWbemLocator,
    IWbemLocator_Vtbl,
    0xdc12a687_737f_11cf_884d_00aa004b2e24
);
windows_core::imp::interface_hierarchy!(IWbemLocator, windows_core::IUnknown);
impl IWbemLocator {
    pub unsafe fn ConnectServer<P6>(
        &self,
        strnetworkresource: &windows_core::BSTR,
        struser: &windows_core::BSTR,
        strpassword: &windows_core::BSTR,
        strlocale: &windows_core::BSTR,
        lsecurityflags: i32,
        strauthority: &windows_core::BSTR,
        pctx: P6,
    ) -> windows_core::Result<IWbemServices>
    where
        P6: windows_core::Param<IWbemContext>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ConnectServer)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strnetworkresource),
                core::mem::transmute_copy(struser),
                core::mem::transmute_copy(strpassword),
                core::mem::transmute_copy(strlocale),
                lsecurityflags,
                core::mem::transmute_copy(strauthority),
                pctx.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
#[repr(C)]
pub struct IWbemLocator_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub ConnectServer: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
pub trait IWbemLocator_Impl: windows_core::IUnknownImpl {
    fn ConnectServer(
        &self,
        strnetworkresource: &windows_core::BSTR,
        struser: &windows_core::BSTR,
        strpassword: &windows_core::BSTR,
        strlocale: &windows_core::BSTR,
        lsecurityflags: i32,
        strauthority: &windows_core::BSTR,
        pctx: windows_core::Ref<IWbemContext>,
    ) -> windows_core::Result<IWbemServices>;
}
impl IWbemLocator_Vtbl {
    pub const fn new<Identity: IWbemLocator_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn ConnectServer<
            Identity: IWbemLocator_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strnetworkresource: *mut core::ffi::c_void,
            struser: *mut core::ffi::c_void,
            strpassword: *mut core::ffi::c_void,
            strlocale: *mut core::ffi::c_void,
            lsecurityflags: i32,
            strauthority: *mut core::ffi::c_void,
            pctx: *mut core::ffi::c_void,
            ppnamespace: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemLocator_Impl::ConnectServer(
                    this,
                    core::mem::transmute(&strnetworkresource),
                    core::mem::transmute(&struser),
                    core::mem::transmute(&strpassword),
                    core::mem::transmute(&strlocale),
                    core::mem::transmute_copy(&lsecurityflags),
                    core::mem::transmute(&strauthority),
                    core::mem::transmute_copy(&pctx),
                ) {
                    Ok(ok__) => {
                        ppnamespace.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            ConnectServer: ConnectServer::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IWbemLocator as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IWbemLocator {}
windows_core::imp::define_interface!(
    IWbemObjectSink,
    IWbemObjectSink_Vtbl,
    0x7c857801_7381_11cf_884d_00aa004b2e24
);
windows_core::imp::interface_hierarchy!(IWbemObjectSink, windows_core::IUnknown);
#[repr(C)]
pub struct IWbemObjectSink_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    Indicate: usize,
    SetStatus: usize,
}
impl windows_core::RuntimeName for IWbemObjectSink {}
windows_core::imp::define_interface!(
    IWbemQualifierSet,
    IWbemQualifierSet_Vtbl,
    0xdc12a680_737f_11cf_884d_00aa004b2e24
);
windows_core::imp::interface_hierarchy!(IWbemQualifierSet, windows_core::IUnknown);
#[repr(C)]
pub struct IWbemQualifierSet_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    Get: usize,
    Put: usize,
    Delete: usize,
    GetNames: usize,
    BeginEnumeration: usize,
    Next: usize,
    EndEnumeration: usize,
}
impl windows_core::RuntimeName for IWbemQualifierSet {}
windows_core::imp::define_interface!(
    IWbemServices,
    IWbemServices_Vtbl,
    0x9556dc99_828c_11cf_a37e_00aa003240c7
);
windows_core::imp::interface_hierarchy!(IWbemServices, windows_core::IUnknown);
impl IWbemServices {
    pub unsafe fn OpenNamespace<P2>(
        &self,
        strnamespace: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
        ppworkingnamespace: *mut Option<Self>,
        ppresult: *mut Option<IWbemCallResult>,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<IWbemContext>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).OpenNamespace)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strnamespace),
                lflags,
                pctx.param().abi(),
                core::mem::transmute(ppworkingnamespace),
                core::mem::transmute(ppresult),
            )
        }
    }
    pub unsafe fn CancelAsyncCall<P0>(&self, psink: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).CancelAsyncCall)(
                windows_core::Interface::as_raw(self),
                psink.param().abi(),
            )
        }
    }
    pub unsafe fn QueryObjectSink(&self, lflags: i32) -> windows_core::Result<IWbemObjectSink> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).QueryObjectSink)(
                windows_core::Interface::as_raw(self),
                lflags,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn GetObject<P2>(
        &self,
        strobjectpath: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
        ppobject: *mut Option<IWbemClassObject>,
        ppcallresult: *mut Option<IWbemCallResult>,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<IWbemContext>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).GetObject)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strobjectpath),
                lflags,
                pctx.param().abi(),
                core::mem::transmute(ppobject),
                core::mem::transmute(ppcallresult),
            )
        }
    }
    pub unsafe fn GetObjectAsync<P2, P3>(
        &self,
        strobjectpath: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
        presponsehandler: P3,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<IWbemContext>,
        P3: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).GetObjectAsync)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strobjectpath),
                lflags,
                pctx.param().abi(),
                presponsehandler.param().abi(),
            )
        }
    }
    pub unsafe fn PutClass<P0, P2>(
        &self,
        pobject: P0,
        lflags: i32,
        pctx: P2,
        ppcallresult: *mut Option<IWbemCallResult>,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IWbemClassObject>,
        P2: windows_core::Param<IWbemContext>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).PutClass)(
                windows_core::Interface::as_raw(self),
                pobject.param().abi(),
                lflags,
                pctx.param().abi(),
                core::mem::transmute(ppcallresult),
            )
        }
    }
    pub unsafe fn PutClassAsync<P0, P2, P3>(
        &self,
        pobject: P0,
        lflags: i32,
        pctx: P2,
        presponsehandler: P3,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IWbemClassObject>,
        P2: windows_core::Param<IWbemContext>,
        P3: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).PutClassAsync)(
                windows_core::Interface::as_raw(self),
                pobject.param().abi(),
                lflags,
                pctx.param().abi(),
                presponsehandler.param().abi(),
            )
        }
    }
    pub unsafe fn DeleteClass<P2>(
        &self,
        strclass: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
        ppcallresult: *mut Option<IWbemCallResult>,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<IWbemContext>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).DeleteClass)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strclass),
                lflags,
                pctx.param().abi(),
                core::mem::transmute(ppcallresult),
            )
        }
    }
    pub unsafe fn DeleteClassAsync<P2, P3>(
        &self,
        strclass: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
        presponsehandler: P3,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<IWbemContext>,
        P3: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).DeleteClassAsync)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strclass),
                lflags,
                pctx.param().abi(),
                presponsehandler.param().abi(),
            )
        }
    }
    pub unsafe fn CreateClassEnum<P2>(
        &self,
        strsuperclass: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
    ) -> windows_core::Result<IEnumWbemClassObject>
    where
        P2: windows_core::Param<IWbemContext>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateClassEnum)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strsuperclass),
                lflags,
                pctx.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateClassEnumAsync<P2, P3>(
        &self,
        strsuperclass: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
        presponsehandler: P3,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<IWbemContext>,
        P3: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).CreateClassEnumAsync)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strsuperclass),
                lflags,
                pctx.param().abi(),
                presponsehandler.param().abi(),
            )
        }
    }
    pub unsafe fn PutInstance<P0, P2>(
        &self,
        pinst: P0,
        lflags: i32,
        pctx: P2,
        ppcallresult: *mut Option<IWbemCallResult>,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IWbemClassObject>,
        P2: windows_core::Param<IWbemContext>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).PutInstance)(
                windows_core::Interface::as_raw(self),
                pinst.param().abi(),
                lflags,
                pctx.param().abi(),
                core::mem::transmute(ppcallresult),
            )
        }
    }
    pub unsafe fn PutInstanceAsync<P0, P2, P3>(
        &self,
        pinst: P0,
        lflags: i32,
        pctx: P2,
        presponsehandler: P3,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IWbemClassObject>,
        P2: windows_core::Param<IWbemContext>,
        P3: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).PutInstanceAsync)(
                windows_core::Interface::as_raw(self),
                pinst.param().abi(),
                lflags,
                pctx.param().abi(),
                presponsehandler.param().abi(),
            )
        }
    }
    pub unsafe fn DeleteInstance<P2>(
        &self,
        strobjectpath: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
        ppcallresult: *mut Option<IWbemCallResult>,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<IWbemContext>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).DeleteInstance)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strobjectpath),
                lflags,
                pctx.param().abi(),
                core::mem::transmute(ppcallresult),
            )
        }
    }
    pub unsafe fn DeleteInstanceAsync<P2, P3>(
        &self,
        strobjectpath: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
        presponsehandler: P3,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<IWbemContext>,
        P3: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).DeleteInstanceAsync)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strobjectpath),
                lflags,
                pctx.param().abi(),
                presponsehandler.param().abi(),
            )
        }
    }
    pub unsafe fn CreateInstanceEnum<P2>(
        &self,
        strfilter: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
    ) -> windows_core::Result<IEnumWbemClassObject>
    where
        P2: windows_core::Param<IWbemContext>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).CreateInstanceEnum)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strfilter),
                lflags,
                pctx.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn CreateInstanceEnumAsync<P2, P3>(
        &self,
        strfilter: &windows_core::BSTR,
        lflags: i32,
        pctx: P2,
        presponsehandler: P3,
    ) -> windows_core::HRESULT
    where
        P2: windows_core::Param<IWbemContext>,
        P3: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).CreateInstanceEnumAsync)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strfilter),
                lflags,
                pctx.param().abi(),
                presponsehandler.param().abi(),
            )
        }
    }
    pub unsafe fn ExecQuery<P3>(
        &self,
        strquerylanguage: &windows_core::BSTR,
        strquery: &windows_core::BSTR,
        lflags: i32,
        pctx: P3,
    ) -> windows_core::Result<IEnumWbemClassObject>
    where
        P3: windows_core::Param<IWbemContext>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ExecQuery)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strquerylanguage),
                core::mem::transmute_copy(strquery),
                lflags,
                pctx.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn ExecQueryAsync<P3, P4>(
        &self,
        strquerylanguage: &windows_core::BSTR,
        strquery: &windows_core::BSTR,
        lflags: i32,
        pctx: P3,
        presponsehandler: P4,
    ) -> windows_core::HRESULT
    where
        P3: windows_core::Param<IWbemContext>,
        P4: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).ExecQueryAsync)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strquerylanguage),
                core::mem::transmute_copy(strquery),
                lflags,
                pctx.param().abi(),
                presponsehandler.param().abi(),
            )
        }
    }
    pub unsafe fn ExecNotificationQuery<P3>(
        &self,
        strquerylanguage: &windows_core::BSTR,
        strquery: &windows_core::BSTR,
        lflags: i32,
        pctx: P3,
    ) -> windows_core::Result<IEnumWbemClassObject>
    where
        P3: windows_core::Param<IWbemContext>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).ExecNotificationQuery)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strquerylanguage),
                core::mem::transmute_copy(strquery),
                lflags,
                pctx.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub unsafe fn ExecNotificationQueryAsync<P3, P4>(
        &self,
        strquerylanguage: &windows_core::BSTR,
        strquery: &windows_core::BSTR,
        lflags: i32,
        pctx: P3,
        presponsehandler: P4,
    ) -> windows_core::HRESULT
    where
        P3: windows_core::Param<IWbemContext>,
        P4: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).ExecNotificationQueryAsync)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strquerylanguage),
                core::mem::transmute_copy(strquery),
                lflags,
                pctx.param().abi(),
                presponsehandler.param().abi(),
            )
        }
    }
    pub unsafe fn ExecMethod<P3, P4>(
        &self,
        strobjectpath: &windows_core::BSTR,
        strmethodname: &windows_core::BSTR,
        lflags: i32,
        pctx: P3,
        pinparams: P4,
        ppoutparams: *mut Option<IWbemClassObject>,
        ppcallresult: *mut Option<IWbemCallResult>,
    ) -> windows_core::HRESULT
    where
        P3: windows_core::Param<IWbemContext>,
        P4: windows_core::Param<IWbemClassObject>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).ExecMethod)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strobjectpath),
                core::mem::transmute_copy(strmethodname),
                lflags,
                pctx.param().abi(),
                pinparams.param().abi(),
                core::mem::transmute(ppoutparams),
                core::mem::transmute(ppcallresult),
            )
        }
    }
    pub unsafe fn ExecMethodAsync<P3, P4, P5>(
        &self,
        strobjectpath: &windows_core::BSTR,
        strmethodname: &windows_core::BSTR,
        lflags: i32,
        pctx: P3,
        pinparams: P4,
        presponsehandler: P5,
    ) -> windows_core::HRESULT
    where
        P3: windows_core::Param<IWbemContext>,
        P4: windows_core::Param<IWbemClassObject>,
        P5: windows_core::Param<IWbemObjectSink>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).ExecMethodAsync)(
                windows_core::Interface::as_raw(self),
                core::mem::transmute_copy(strobjectpath),
                core::mem::transmute_copy(strmethodname),
                lflags,
                pctx.param().abi(),
                pinparams.param().abi(),
                presponsehandler.param().abi(),
            )
        }
    }
}
#[repr(C)]
pub struct IWbemServices_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub OpenNamespace: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CancelAsyncCall: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub QueryObjectSink: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        i32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetObject: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetObjectAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub PutClass: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub PutClassAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub DeleteClass: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub DeleteClassAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateClassEnum: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateClassEnumAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub PutInstance: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub PutInstanceAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub DeleteInstance: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub DeleteInstanceAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateInstanceEnum: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub CreateInstanceEnumAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ExecQuery: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ExecQueryAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ExecNotificationQuery: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ExecNotificationQueryAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ExecMethod: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ExecMethodAsync: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
pub trait IWbemServices_Impl: windows_core::IUnknownImpl {
    fn OpenNamespace(
        &self,
        strnamespace: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        ppworkingnamespace: windows_core::OutRef<IWbemServices>,
        ppresult: windows_core::OutRef<IWbemCallResult>,
    ) -> windows_core::Result<()>;
    fn CancelAsyncCall(
        &self,
        psink: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn QueryObjectSink(&self, lflags: i32) -> windows_core::Result<IWbemObjectSink>;
    fn GetObject(
        &self,
        strobjectpath: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        ppobject: windows_core::OutRef<IWbemClassObject>,
        ppcallresult: windows_core::OutRef<IWbemCallResult>,
    ) -> windows_core::Result<()>;
    fn GetObjectAsync(
        &self,
        strobjectpath: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        presponsehandler: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn PutClass(
        &self,
        pobject: windows_core::Ref<IWbemClassObject>,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        ppcallresult: windows_core::OutRef<IWbemCallResult>,
    ) -> windows_core::Result<()>;
    fn PutClassAsync(
        &self,
        pobject: windows_core::Ref<IWbemClassObject>,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        presponsehandler: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn DeleteClass(
        &self,
        strclass: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        ppcallresult: windows_core::OutRef<IWbemCallResult>,
    ) -> windows_core::Result<()>;
    fn DeleteClassAsync(
        &self,
        strclass: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        presponsehandler: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn CreateClassEnum(
        &self,
        strsuperclass: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
    ) -> windows_core::Result<IEnumWbemClassObject>;
    fn CreateClassEnumAsync(
        &self,
        strsuperclass: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        presponsehandler: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn PutInstance(
        &self,
        pinst: windows_core::Ref<IWbemClassObject>,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        ppcallresult: windows_core::OutRef<IWbemCallResult>,
    ) -> windows_core::Result<()>;
    fn PutInstanceAsync(
        &self,
        pinst: windows_core::Ref<IWbemClassObject>,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        presponsehandler: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn DeleteInstance(
        &self,
        strobjectpath: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        ppcallresult: windows_core::OutRef<IWbemCallResult>,
    ) -> windows_core::Result<()>;
    fn DeleteInstanceAsync(
        &self,
        strobjectpath: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        presponsehandler: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn CreateInstanceEnum(
        &self,
        strfilter: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
    ) -> windows_core::Result<IEnumWbemClassObject>;
    fn CreateInstanceEnumAsync(
        &self,
        strfilter: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        presponsehandler: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn ExecQuery(
        &self,
        strquerylanguage: &windows_core::BSTR,
        strquery: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
    ) -> windows_core::Result<IEnumWbemClassObject>;
    fn ExecQueryAsync(
        &self,
        strquerylanguage: &windows_core::BSTR,
        strquery: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        presponsehandler: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn ExecNotificationQuery(
        &self,
        strquerylanguage: &windows_core::BSTR,
        strquery: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
    ) -> windows_core::Result<IEnumWbemClassObject>;
    fn ExecNotificationQueryAsync(
        &self,
        strquerylanguage: &windows_core::BSTR,
        strquery: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        presponsehandler: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
    fn ExecMethod(
        &self,
        strobjectpath: &windows_core::BSTR,
        strmethodname: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        pinparams: windows_core::Ref<IWbemClassObject>,
        ppoutparams: windows_core::OutRef<IWbemClassObject>,
        ppcallresult: windows_core::OutRef<IWbemCallResult>,
    ) -> windows_core::Result<()>;
    fn ExecMethodAsync(
        &self,
        strobjectpath: &windows_core::BSTR,
        strmethodname: &windows_core::BSTR,
        lflags: i32,
        pctx: windows_core::Ref<IWbemContext>,
        pinparams: windows_core::Ref<IWbemClassObject>,
        presponsehandler: windows_core::Ref<IWbemObjectSink>,
    ) -> windows_core::Result<()>;
}
impl IWbemServices_Vtbl {
    pub const fn new<Identity: IWbemServices_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn OpenNamespace<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strnamespace: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            ppworkingnamespace: *mut *mut core::ffi::c_void,
            ppresult: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::OpenNamespace(
                    this,
                    core::mem::transmute(&strnamespace),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&ppworkingnamespace),
                    core::mem::transmute_copy(&ppresult),
                )
                .into()
            }
        }
        unsafe extern "system" fn CancelAsyncCall<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            psink: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::CancelAsyncCall(this, core::mem::transmute_copy(&psink)).into()
            }
        }
        unsafe extern "system" fn QueryObjectSink<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            lflags: i32,
            ppresponsehandler: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemServices_Impl::QueryObjectSink(this, core::mem::transmute_copy(&lflags))
                {
                    Ok(ok__) => {
                        ppresponsehandler.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn GetObject<Identity: IWbemServices_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            strobjectpath: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            ppobject: *mut *mut core::ffi::c_void,
            ppcallresult: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::GetObject(
                    this,
                    core::mem::transmute(&strobjectpath),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&ppobject),
                    core::mem::transmute_copy(&ppcallresult),
                )
                .into()
            }
        }
        unsafe extern "system" fn GetObjectAsync<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strobjectpath: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            presponsehandler: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::GetObjectAsync(
                    this,
                    core::mem::transmute(&strobjectpath),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&presponsehandler),
                )
                .into()
            }
        }
        unsafe extern "system" fn PutClass<Identity: IWbemServices_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pobject: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            ppcallresult: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::PutClass(
                    this,
                    core::mem::transmute_copy(&pobject),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&ppcallresult),
                )
                .into()
            }
        }
        unsafe extern "system" fn PutClassAsync<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pobject: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            presponsehandler: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::PutClassAsync(
                    this,
                    core::mem::transmute_copy(&pobject),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&presponsehandler),
                )
                .into()
            }
        }
        unsafe extern "system" fn DeleteClass<Identity: IWbemServices_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            strclass: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            ppcallresult: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::DeleteClass(
                    this,
                    core::mem::transmute(&strclass),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&ppcallresult),
                )
                .into()
            }
        }
        unsafe extern "system" fn DeleteClassAsync<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strclass: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            presponsehandler: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::DeleteClassAsync(
                    this,
                    core::mem::transmute(&strclass),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&presponsehandler),
                )
                .into()
            }
        }
        unsafe extern "system" fn CreateClassEnum<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strsuperclass: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            ppenum: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemServices_Impl::CreateClassEnum(
                    this,
                    core::mem::transmute(&strsuperclass),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                ) {
                    Ok(ok__) => {
                        ppenum.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn CreateClassEnumAsync<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strsuperclass: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            presponsehandler: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::CreateClassEnumAsync(
                    this,
                    core::mem::transmute(&strsuperclass),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&presponsehandler),
                )
                .into()
            }
        }
        unsafe extern "system" fn PutInstance<Identity: IWbemServices_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pinst: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            ppcallresult: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::PutInstance(
                    this,
                    core::mem::transmute_copy(&pinst),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&ppcallresult),
                )
                .into()
            }
        }
        unsafe extern "system" fn PutInstanceAsync<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pinst: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            presponsehandler: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::PutInstanceAsync(
                    this,
                    core::mem::transmute_copy(&pinst),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&presponsehandler),
                )
                .into()
            }
        }
        unsafe extern "system" fn DeleteInstance<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strobjectpath: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            ppcallresult: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::DeleteInstance(
                    this,
                    core::mem::transmute(&strobjectpath),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&ppcallresult),
                )
                .into()
            }
        }
        unsafe extern "system" fn DeleteInstanceAsync<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strobjectpath: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            presponsehandler: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::DeleteInstanceAsync(
                    this,
                    core::mem::transmute(&strobjectpath),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&presponsehandler),
                )
                .into()
            }
        }
        unsafe extern "system" fn CreateInstanceEnum<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strfilter: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            ppenum: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemServices_Impl::CreateInstanceEnum(
                    this,
                    core::mem::transmute(&strfilter),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                ) {
                    Ok(ok__) => {
                        ppenum.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn CreateInstanceEnumAsync<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strfilter: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            presponsehandler: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::CreateInstanceEnumAsync(
                    this,
                    core::mem::transmute(&strfilter),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&presponsehandler),
                )
                .into()
            }
        }
        unsafe extern "system" fn ExecQuery<Identity: IWbemServices_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            strquerylanguage: *mut core::ffi::c_void,
            strquery: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            ppenum: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemServices_Impl::ExecQuery(
                    this,
                    core::mem::transmute(&strquerylanguage),
                    core::mem::transmute(&strquery),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                ) {
                    Ok(ok__) => {
                        ppenum.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn ExecQueryAsync<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strquerylanguage: *mut core::ffi::c_void,
            strquery: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            presponsehandler: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::ExecQueryAsync(
                    this,
                    core::mem::transmute(&strquerylanguage),
                    core::mem::transmute(&strquery),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&presponsehandler),
                )
                .into()
            }
        }
        unsafe extern "system" fn ExecNotificationQuery<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strquerylanguage: *mut core::ffi::c_void,
            strquery: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            ppenum: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IWbemServices_Impl::ExecNotificationQuery(
                    this,
                    core::mem::transmute(&strquerylanguage),
                    core::mem::transmute(&strquery),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                ) {
                    Ok(ok__) => {
                        ppenum.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn ExecNotificationQueryAsync<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strquerylanguage: *mut core::ffi::c_void,
            strquery: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            presponsehandler: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::ExecNotificationQueryAsync(
                    this,
                    core::mem::transmute(&strquerylanguage),
                    core::mem::transmute(&strquery),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&presponsehandler),
                )
                .into()
            }
        }
        unsafe extern "system" fn ExecMethod<Identity: IWbemServices_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            strobjectpath: *mut core::ffi::c_void,
            strmethodname: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            pinparams: *mut core::ffi::c_void,
            ppoutparams: *mut *mut core::ffi::c_void,
            ppcallresult: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::ExecMethod(
                    this,
                    core::mem::transmute(&strobjectpath),
                    core::mem::transmute(&strmethodname),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&pinparams),
                    core::mem::transmute_copy(&ppoutparams),
                    core::mem::transmute_copy(&ppcallresult),
                )
                .into()
            }
        }
        unsafe extern "system" fn ExecMethodAsync<
            Identity: IWbemServices_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            strobjectpath: *mut core::ffi::c_void,
            strmethodname: *mut core::ffi::c_void,
            lflags: i32,
            pctx: *mut core::ffi::c_void,
            pinparams: *mut core::ffi::c_void,
            presponsehandler: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IWbemServices_Impl::ExecMethodAsync(
                    this,
                    core::mem::transmute(&strobjectpath),
                    core::mem::transmute(&strmethodname),
                    core::mem::transmute_copy(&lflags),
                    core::mem::transmute_copy(&pctx),
                    core::mem::transmute_copy(&pinparams),
                    core::mem::transmute_copy(&presponsehandler),
                )
                .into()
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            OpenNamespace: OpenNamespace::<Identity, OFFSET>,
            CancelAsyncCall: CancelAsyncCall::<Identity, OFFSET>,
            QueryObjectSink: QueryObjectSink::<Identity, OFFSET>,
            GetObject: GetObject::<Identity, OFFSET>,
            GetObjectAsync: GetObjectAsync::<Identity, OFFSET>,
            PutClass: PutClass::<Identity, OFFSET>,
            PutClassAsync: PutClassAsync::<Identity, OFFSET>,
            DeleteClass: DeleteClass::<Identity, OFFSET>,
            DeleteClassAsync: DeleteClassAsync::<Identity, OFFSET>,
            CreateClassEnum: CreateClassEnum::<Identity, OFFSET>,
            CreateClassEnumAsync: CreateClassEnumAsync::<Identity, OFFSET>,
            PutInstance: PutInstance::<Identity, OFFSET>,
            PutInstanceAsync: PutInstanceAsync::<Identity, OFFSET>,
            DeleteInstance: DeleteInstance::<Identity, OFFSET>,
            DeleteInstanceAsync: DeleteInstanceAsync::<Identity, OFFSET>,
            CreateInstanceEnum: CreateInstanceEnum::<Identity, OFFSET>,
            CreateInstanceEnumAsync: CreateInstanceEnumAsync::<Identity, OFFSET>,
            ExecQuery: ExecQuery::<Identity, OFFSET>,
            ExecQueryAsync: ExecQueryAsync::<Identity, OFFSET>,
            ExecNotificationQuery: ExecNotificationQuery::<Identity, OFFSET>,
            ExecNotificationQueryAsync: ExecNotificationQueryAsync::<Identity, OFFSET>,
            ExecMethod: ExecMethod::<Identity, OFFSET>,
            ExecMethodAsync: ExecMethodAsync::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IWbemServices as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IWbemServices {}
pub const IfOperStatusUp: IF_OPER_STATUS = 1;
pub const KEY_READ: i32 = 131097;
pub const KEY_WOW64_64KEY: i32 = 256;
pub const KEY_WRITE: i32 = 131078;
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LOGFONTW {
    pub lfHeight: i32,
    pub lfWidth: i32,
    pub lfEscapement: i32,
    pub lfOrientation: i32,
    pub lfWeight: i32,
    pub lfItalic: u8,
    pub lfUnderline: u8,
    pub lfStrikeOut: u8,
    pub lfCharSet: u8,
    pub lfOutPrecision: u8,
    pub lfClipPrecision: u8,
    pub lfQuality: u8,
    pub lfPitchAndFamily: u8,
    pub lfFaceName: [u16; 32],
}
impl Default for LOGFONTW {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
pub const LOGPIXELSX: i32 = 88;
pub const LOGPIXELSY: i32 = 90;
pub type LPDEVMODEW = *mut DEVMODEW;
pub type LPSOCKADDR = *mut SOCKADDR;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LSTATUS(pub i32);
pub const MWT_IDENTITY: i32 = 1;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct NET_IFINDEX(pub u32);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct NET_IF_COMPARTMENT_ID(pub u32);
pub type NET_IF_CONNECTION_TYPE = i32;
pub type NET_IF_NETWORK_GUID = windows_core::GUID;
pub type NET_LUID = NET_LUID_LH;
#[repr(C)]
#[derive(Clone, Copy)]
pub union NET_LUID_LH {
    pub Value: u64,
    pub Info: NET_LUID_LH_0,
}
impl Default for NET_LUID_LH {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NET_LUID_LH_0 {
    pub _bitfield: u64,
}
impl NET_LUID_LH_0 {
    pub fn Reserved(&self) -> u64 {
        (self._bitfield << 40) >> 40
    }
    pub fn set_Reserved(&mut self, value: u64) {
        self._bitfield = (self._bitfield & !16777215) | (value & 16777215);
    }
    pub fn NetLuidIndex(&self) -> u64 {
        (self._bitfield << 16) >> 40
    }
    pub fn set_NetLuidIndex(&mut self, value: u64) {
        self._bitfield = (self._bitfield & !(16777215 << 24)) | ((value & 16777215) << 24);
    }
    pub fn IfType(&self) -> u64 {
        self._bitfield >> 48
    }
    pub fn set_IfType(&mut self, value: u64) {
        self._bitfield = (self._bitfield & !(65535 << 48)) | ((value & 65535) << 48);
    }
}
pub type NL_DAD_STATE = i32;
pub type NL_PREFIX_ORIGIN = i32;
pub type NL_SUFFIX_ORIGIN = i32;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct OLECHAR(pub u16);
pub const OUT_TT_PRECIS: i32 = 4;
pub type PCHAR = *mut i8;
pub const PHYSICALHEIGHT: i32 = 111;
pub const PHYSICALOFFSETX: i32 = 112;
pub const PHYSICALOFFSETY: i32 = 113;
pub const PHYSICALWIDTH: i32 = 110;
pub type PIP_ADAPTER_ANYCAST_ADDRESS_XP = *mut IP_ADAPTER_ANYCAST_ADDRESS_XP;
pub type PIP_ADAPTER_DNS_SERVER_ADDRESS_XP = *mut IP_ADAPTER_DNS_SERVER_ADDRESS_XP;
pub type PIP_ADAPTER_DNS_SUFFIX = *mut IP_ADAPTER_DNS_SUFFIX;
pub type PIP_ADAPTER_GATEWAY_ADDRESS_LH = *mut IP_ADAPTER_GATEWAY_ADDRESS_LH;
pub type PIP_ADAPTER_MULTICAST_ADDRESS_XP = *mut IP_ADAPTER_MULTICAST_ADDRESS_XP;
pub type PIP_ADAPTER_PREFIX_XP = *mut IP_ADAPTER_PREFIX_XP;
pub type PIP_ADAPTER_UNICAST_ADDRESS_LH = *mut IP_ADAPTER_UNICAST_ADDRESS_LH;
pub type PIP_ADAPTER_WINS_SERVER_ADDRESS_LH = *mut IP_ADAPTER_WINS_SERVER_ADDRESS_LH;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct POINTL {
    pub x: i32,
    pub y: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PORT_INFO_2W {
    pub pPortName: windows_core::PWSTR,
    pub pMonitorName: windows_core::PWSTR,
    pub pDescription: windows_core::PWSTR,
    pub fPortType: u32,
    pub Reserved: u32,
}
pub const PRINTER_ATTRIBUTE_FAX: i32 = 16384;
pub const PRINTER_ATTRIBUTE_KEEPPRINTEDJOBS: i32 = 256;
pub const PRINTER_ENUM_CONNECTIONS: i32 = 4;
pub const PRINTER_ENUM_LOCAL: i32 = 2;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PRINTER_INFO_2W {
    pub pServerName: windows_core::PWSTR,
    pub pPrinterName: windows_core::PWSTR,
    pub pShareName: windows_core::PWSTR,
    pub pPortName: windows_core::PWSTR,
    pub pDriverName: windows_core::PWSTR,
    pub pComment: windows_core::PWSTR,
    pub pLocation: windows_core::PWSTR,
    pub pDevMode: LPDEVMODEW,
    pub pSepFile: windows_core::PWSTR,
    pub pPrintProcessor: windows_core::PWSTR,
    pub pDatatype: windows_core::PWSTR,
    pub pParameters: windows_core::PWSTR,
    pub pSecurityDescriptor: PSECURITY_DESCRIPTOR,
    pub Attributes: u32,
    pub Priority: u32,
    pub DefaultPriority: u32,
    pub StartTime: u32,
    pub UntilTime: u32,
    pub Status: u32,
    pub cJobs: u32,
    pub AveragePPM: u32,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PSECURITY_DESCRIPTOR(pub *mut core::ffi::c_void);
pub type PUCHAR = *mut u8;
pub type PWCHAR = *mut u16;
pub type REGSAM = ACCESS_MASK;
pub const REG_BINARY: u32 = 3;
pub const REG_OPTION_NON_VOLATILE: i32 = 0;
pub const REG_SZ: u32 = 1;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RGBQUAD {
    pub rgbBlue: u8,
    pub rgbGreen: u8,
    pub rgbRed: u8,
    pub rgbReserved: u8,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct RPC_AUTH_IDENTITY_HANDLE(pub *mut core::ffi::c_void);
pub const RPC_C_AUTHN_LEVEL_CALL: i32 = 3;
pub const RPC_C_AUTHN_WINNT: i32 = 10;
pub const RPC_C_AUTHZ_NONE: i32 = 0;
pub const RPC_C_IMP_LEVEL_IMPERSONATE: i32 = 3;
pub const RPC_E_CHANGED_MODE: windows_core::HRESULT = windows_core::HRESULT(0x80010106_u32 as _);
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SAFEARRAY {
    pub cDims: u16,
    pub fFeatures: u16,
    pub cbElements: u32,
    pub cLocks: u32,
    pub pvData: *mut core::ffi::c_void,
    pub rgsabound: [SAFEARRAYBOUND; 1],
}
impl Default for SAFEARRAY {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SAFEARRAYBOUND {
    pub cElements: u32,
    pub lLbound: i32,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SCODE(pub i32);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SCOPE_ID {
    pub Anonymous: SCOPE_ID_0,
}
impl Default for SCOPE_ID {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union SCOPE_ID_0 {
    pub Anonymous: SCOPE_ID_0_0,
    pub Value: u32,
}
impl Default for SCOPE_ID_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SCOPE_ID_0_0 {
    pub _bitfield: u32,
}
impl SCOPE_ID_0_0 {
    pub fn Zone(&self) -> u32 {
        (self._bitfield << 4) >> 4
    }
    pub fn set_Zone(&mut self, value: u32) {
        self._bitfield = (self._bitfield & !268435455) | (value & 268435455);
    }
    pub fn Level(&self) -> u32 {
        self._bitfield >> 28
    }
    pub fn set_Level(&mut self, value: u32) {
        self._bitfield = (self._bitfield & !(15 << 28)) | ((value & 15) << 28);
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SECURITY_ATTRIBUTES {
    pub nLength: u32,
    pub lpSecurityDescriptor: *mut core::ffi::c_void,
    pub bInheritHandle: windows_core::BOOL,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SOCKADDR {
    pub sa_family: ADDRESS_FAMILY,
    pub sa_data: [i8; 14],
}
impl Default for SOCKADDR {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SOCKADDR_IN {
    pub sin_family: ADDRESS_FAMILY,
    pub sin_port: u16,
    pub sin_addr: IN_ADDR,
    pub sin_zero: [i8; 8],
}
impl Default for SOCKADDR_IN {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
pub type SOCKADDR_IN6 = SOCKADDR_IN6_LH;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SOCKADDR_IN6_LH {
    pub sin6_family: ADDRESS_FAMILY,
    pub sin6_port: u16,
    pub sin6_flowinfo: u32,
    pub sin6_addr: IN6_ADDR,
    pub Anonymous: SOCKADDR_IN6_LH_0,
}
impl Default for SOCKADDR_IN6_LH {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union SOCKADDR_IN6_LH_0 {
    pub sin6_scope_id: u32,
    pub sin6_scope_struct: SCOPE_ID,
}
impl Default for SOCKADDR_IN6_LH_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SOCKET_ADDRESS {
    pub lpSockaddr: LPSOCKADDR,
    pub iSockaddrLength: i32,
}
pub const SRCCOPY: u32 = 13369376;
pub const STATUS_AUTH_TAG_MISMATCH: windows_core::NTSTATUS =
    windows_core::NTSTATUS(0xC000A002_u32 as _);
pub const TA_BASELINE: i32 = 24;
pub const TA_CENTER: i32 = 6;
pub const TECHNOLOGY: i32 = 2;
pub const TRANSPARENT: i32 = 1;
pub type TUNNEL_TYPE = i32;
pub type VARENUM = i32;
#[repr(C)]
pub struct VARIANT {
    pub Anonymous: VARIANT_0,
}
impl Clone for VARIANT {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub union VARIANT_0 {
    pub Anonymous: core::mem::ManuallyDrop<VARIANT_0_0>,
    pub decVal: DECIMAL,
}
impl Clone for VARIANT_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub struct VARIANT_0_0 {
    pub vt: VARTYPE,
    pub wReserved1: u16,
    pub wReserved2: u16,
    pub wReserved3: u16,
    pub Anonymous: VARIANT_0_0_0,
}
impl Clone for VARIANT_0_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT_0_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub union VARIANT_0_0_0 {
    pub llVal: i64,
    pub lVal: i32,
    pub bVal: u8,
    pub iVal: i16,
    pub fltVal: f32,
    pub dblVal: f64,
    pub boolVal: VARIANT_BOOL,
    pub __OBSOLETE__VARIANT_BOOL: VARIANT_BOOL,
    pub scode: SCODE,
    pub cyVal: CY,
    pub date: f64,
    pub bstrVal: core::mem::ManuallyDrop<windows_core::BSTR>,
    pub punkVal: core::mem::ManuallyDrop<Option<windows_core::IUnknown>>,
    pub pdispVal: core::mem::ManuallyDrop<Option<IDispatch>>,
    pub parray: *mut SAFEARRAY,
    pub pbVal: *mut u8,
    pub piVal: *mut i16,
    pub plVal: *mut i32,
    pub pllVal: *mut i64,
    pub pfltVal: *mut f32,
    pub pdblVal: *mut f64,
    pub pboolVal: *mut VARIANT_BOOL,
    pub __OBSOLETE__VARIANT_PBOOL: *mut VARIANT_BOOL,
    pub pscode: *mut SCODE,
    pub pcyVal: *mut CY,
    pub pdate: *mut f64,
    pub pbstrVal: *mut windows_core::BSTR,
    pub ppunkVal: *mut Option<windows_core::IUnknown>,
    pub ppdispVal: *mut Option<IDispatch>,
    pub pparray: *mut *mut SAFEARRAY,
    pub pvarVal: *mut VARIANT,
    pub byref: *mut core::ffi::c_void,
    pub cVal: i8,
    pub uiVal: u16,
    pub ulVal: u32,
    pub ullVal: u64,
    pub intVal: i32,
    pub uintVal: u32,
    pub pdecVal: *mut DECIMAL,
    pub pcVal: *mut i8,
    pub puiVal: *mut u16,
    pub pulVal: *mut u32,
    pub pullVal: *mut u64,
    pub pintVal: *mut i32,
    pub puintVal: *mut u32,
    pub Anonymous: core::mem::ManuallyDrop<VARIANT_0_0_0_0>,
}
impl Clone for VARIANT_0_0_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT_0_0_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VARIANT_0_0_0_0 {
    pub pvRecord: *mut core::ffi::c_void,
    pub pRecInfo: core::mem::ManuallyDrop<Option<IRecordInfo>>,
}
pub type VARIANTARG = VARIANT;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VARIANT_BOOL(pub i16);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VARTYPE(pub u16);
pub const VERTRES: i32 = 10;
pub const VT_BSTR: VARENUM = 8;
pub const WAIT_ABANDONED: i32 = 128;
pub const WAIT_OBJECT_0: i32 = 0;
pub const WAIT_TIMEOUT: i32 = 258;
pub const WBEM_FLAG_FORWARD_ONLY: WBEM_GENERIC_FLAG_TYPE = 32;
pub const WBEM_FLAG_RETURN_IMMEDIATELY: WBEM_GENERIC_FLAG_TYPE = 16;
pub type WBEM_GENERIC_FLAG_TYPE = i32;
pub const WBEM_INFINITE: WBEM_TIMEOUT_TYPE = -1;
pub type WBEM_TIMEOUT_TYPE = i32;
pub const WbemLocator: windows_core::GUID =
    windows_core::GUID::from_u128(0x4590f811_1d3a_11d0_891f_00aa004b2e24);
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XFORM {
    pub eM11: f32,
    pub eM12: f32,
    pub eM21: f32,
    pub eM22: f32,
    pub eDx: f32,
    pub eDy: f32,
}
