//! Exercise binder's public device ABI from an AArch64 guest.
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn binder_contexts_and_protocol_version() {
    let root = std::env::temp_dir().join(format!("indroid-binder-test-{}", std::process::id()));
    let rom = root.join("rom");
    std::fs::create_dir_all(rom.join("bin")).unwrap();
    let source = root.join("binder.c");
    std::fs::write(&source, r#"
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>
#include <errno.h>
#include <string.h>
struct write_read { uint64_t ws, wc, wp, rs, rc, rp; };
struct txn { uint64_t target, cookie; uint32_t code, flags, pid, uid; uint64_t size, offsets_size, buffer, offsets; };
#define VERSION _IOWR('b', 9, int)
#define MANAGER _IOW('b', 7, int)
#define WRITE_READ _IOWR('b', 1, struct write_read)
int main(void) {
 int a=open("/dev/binder", O_RDWR|O_NONBLOCK), b=open("/dev/binder", O_RDWR), c=open("/dev/hwbinder", O_RDWR), d=open("/dev/vndbinder", O_RDWR);
 if (a<0||b<0||c<0||d<0) return puts("open failed"),1;
 int v=0; if (ioctl(a,VERSION,&v)||v!=8) return puts("version failed"),2;
 if (ioctl(a,MANAGER,0) || ioctl(c,MANAGER,0) || ioctl(d,MANAGER,0)) return puts("manager failed"),3;
 if (ioctl(b,MANAGER,0)!=-1 || errno!=EBUSY) return puts("duplicate failed"),4;
 void *p=mmap(0, 65536, PROT_READ, MAP_PRIVATE, a, 0);
 if (mmap(0, 65536, PROT_READ, MAP_PRIVATE, b, 0)==MAP_FAILED) return puts("client map failed"),17;
 if (p==MAP_FAILED) return puts("map failed"),5;
 uint32_t result=0; struct write_read wr={0,0,0,4,0,(uintptr_t)&result};
 if (ioctl(a,WRITE_READ,&wr) || wr.rc!=4 || result!=_IO('r',12)) return puts("nonblock failed"),6;

 unsigned char cmd[76], result_buf[128];
 /* Like Linux, only looper threads take process work: BC_ENTER_LOOPER. */
 { uint32_t enter=_IO('c',12); struct write_read lw={4,0,(uintptr_t)&enter,0,0,0};
   if (ioctl(a,WRITE_READ,&lw)) return puts("enter looper failed"),18; }
 struct txn tx={0}; tx.code=42; tx.size=3; tx.buffer=(uintptr_t)"abc";
 uint32_t op=_IOW('c',0,struct txn); memcpy(cmd,&op,4); memcpy(cmd+4,&tx,64);
 wr=(struct write_read){68,0,(uintptr_t)cmd,0,0,0};
 if (ioctl(b,WRITE_READ,&wr)||wr.wc!=68) return puts("send failed"),8;
 wr=(struct write_read){0,0,0,sizeof(result_buf),0,(uintptr_t)result_buf};
 if (ioctl(a,WRITE_READ,&wr)||wr.rc!=68) return puts("receive failed"),9;
 memcpy(&op,result_buf,4); memcpy(&tx,result_buf+4,64);
 if (op!=_IOR('r',2,struct txn)||tx.code!=42||tx.size!=3||memcmp((void*)(uintptr_t)tx.buffer,"abc",3)) return puts("payload failed"),10;
 op=_IOW('c',3,uint64_t); memcpy(cmd,&op,4); memcpy(cmd+4,&tx.buffer,8);
 wr=(struct write_read){12,0,(uintptr_t)cmd,0,0,0};
 if (ioctl(a,WRITE_READ,&wr)) return puts("free failed"),11;
 memset(&tx,0,sizeof(tx)); tx.size=2; tx.buffer=(uintptr_t)"ok";
 op=_IOW('c',1,struct txn); memcpy(cmd,&op,4); memcpy(cmd+4,&tx,64);
 wr=(struct write_read){68,0,(uintptr_t)cmd,0,0,0};
 if (ioctl(a,WRITE_READ,&wr)) return puts("reply failed"),12;
 wr=(struct write_read){0,0,0,sizeof(result_buf),0,(uintptr_t)result_buf};
 if (ioctl(b,WRITE_READ,&wr)||wr.rc!=4) return puts("completion failed"),13;
 memcpy(&op,result_buf,4); if (op!=_IO('r',6)) return puts("completion code failed"),14;
 wr.rc=0; if (ioctl(b,WRITE_READ,&wr)||wr.rc!=68) return puts("reply read failed"),15;
 memcpy(&op,result_buf,4); memcpy(&tx,result_buf+4,64);
 if (op!=_IOR('r',3,struct txn)||tx.size!=2||memcmp((void*)(uintptr_t)tx.buffer,"ok",2)) return puts("reply payload failed"),16;

 // Send a local Binder, an fd, and a HIDL-style buffer in one SG call.
 struct { uint32_t type, flags; uint64_t binder, cookie; } obj={0x73622a85,0x100,0x12340,0x56780};
 struct { uint32_t type, flags; uint64_t fd, cookie; } fobj={0x66642a85,0,0,0};
 struct { uint32_t type, flags; uint64_t ptr,len,parent,parent_off; } pobj={0x70742a85,0,0,5,0,0};
 int source=open("/dev/null",O_RDWR); fobj.fd=source; pobj.ptr=(uintptr_t)"hidl";
 unsigned char parcel[92]={0}; memcpy(parcel+4,&obj,24); memcpy(parcel+28,&fobj,24); memcpy(parcel+52,&pobj,40);
 uint64_t offsets[]={4,28,52}; memset(&tx,0,sizeof(tx)); tx.flags=1; tx.size=92; tx.offsets_size=sizeof(offsets); tx.buffer=(uintptr_t)parcel; tx.offsets=(uintptr_t)offsets;
 op=_IOC(_IOC_WRITE,'c',17,72); memcpy(cmd,&op,4); memcpy(cmd+4,&tx,64); uint64_t sglen=8; memcpy(cmd+68,&sglen,8);
 wr=(struct write_read){76,0,(uintptr_t)cmd,0,0,0}; if (ioctl(b,WRITE_READ,&wr)) return puts("sg send failed"),18;
 wr=(struct write_read){0,0,0,sizeof(result_buf),0,(uintptr_t)result_buf};
 if (ioctl(a,WRITE_READ,&wr)) return puts("sg first read failed"),19;
 memcpy(&op,result_buf,4); if (op==_IO('r',6)) { wr.rc=0; if(ioctl(a,WRITE_READ,&wr)) return puts("sg second read failed"),20; memcpy(&op,result_buf,4); }
 if (op!=_IOR('r',2,struct txn)) return puts("sg transaction missing"),21;
 memcpy(&tx,result_buf+4,64); memcpy(parcel,(void*)(uintptr_t)tx.buffer,92); memcpy(&obj,parcel+4,24); memcpy(&fobj,parcel+28,24); memcpy(&pobj,parcel+52,40);
 if (obj.type!=0x73682a85 || obj.binder!=1 || fobj.fd==(uint64_t)source || fcntl((int)fobj.fd,F_GETFD)!=FD_CLOEXEC || memcmp((void*)(uintptr_t)pobj.ptr,"hidl",5)) return puts("sg translation failed"),22;

 // Death delivery after the owner of a translated handle closes its driver fd.
 struct __attribute__((packed)) { uint32_t h; uint64_t cookie; } death={1,0xabcdef};
 op=_IOC(_IOC_WRITE,'c',14,12); memcpy(cmd,&op,4); memcpy(cmd+4,&death,12);
 wr=(struct write_read){16,0,(uintptr_t)cmd,0,0,0}; if (ioctl(a,WRITE_READ,&wr)) return puts("death request failed"),23;
 close(b);
 wr=(struct write_read){0,0,0,sizeof(result_buf),0,(uintptr_t)result_buf};
 if (ioctl(a,WRITE_READ,&wr) || wr.rc!=12) return puts("death read failed"),24;
 memcpy(&op,result_buf,4); uint64_t death_cookie; memcpy(&death_cookie,result_buf+4,8);
 if (op!=_IOR('r',15,uint64_t) || death_cookie!=death.cookie) return puts("death notification failed"),25;
 int e=open("/dev/binder",O_RDWR); if(e<0) return puts("reopen failed"),26;
 close(a); if (ioctl(e,MANAGER,0)) return puts("release failed"),7;
 puts("binder basic ok"); return 0;
}
"#).unwrap();
    let build = Command::new("aarch64-linux-gnu-gcc")
        .args(["-static", "-O2", "-o"])
        .arg(rom.join("bin/binder"))
        .arg(&source)
        .status();
    match build {
        Err(_) => {
            eprintln!("skipping: aarch64-linux-gnu-gcc unavailable");
            return;
        }
        Ok(s) => assert!(s.success()),
    }
    let out = Arc::new(Mutex::new(Vec::new()));
    let sink = out.clone();
    let kernel = indroid_kernel::Kernel::new(indroid_kernel::Config {
        rom,
        overlays: vec![],
        state: root.join("state"),
        console: Arc::new(move |b| sink.lock().unwrap().extend_from_slice(b)),
    });
    kernel.spawn("/bin/binder", &["/bin/binder"], &[]).unwrap();
    assert!(
        kernel.wait_idle(Some(Duration::from_secs(60))),
        "guest timed out"
    );
    let output = String::from_utf8_lossy(&out.lock().unwrap()).into_owned();
    assert!(output.contains("binder basic ok"), "{output}");
    let _ = std::fs::remove_dir_all(root);
}

/// Opt in when the unpacked Android 13 ROM is available on the host.
#[test]
#[ignore = "requires the local AOSP Android 13 ROM"]
fn android_servicemanager_and_service_list() {
    use std::sync::atomic::Ordering;
    let rom = std::path::PathBuf::from(
        std::env::var("INDROID_TEST_ROM")
            .unwrap_or_else(|_| format!("{}/aosp/rom13", std::env::var("HOME").unwrap())),
    );
    let root = std::env::temp_dir().join(format!("indroid-binder-rom-{}", std::process::id()));
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let sink = output.clone();
    let kernel = indroid_kernel::Kernel::new(indroid_kernel::Config {
        rom,
        overlays: vec![
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rom-overlay")
        ],
        state: root,
        console: Arc::new(move |data| sink.lock().unwrap().extend_from_slice(data)),
    });
    let env = [
        "PATH=/system/bin:/apex/com.android.runtime/bin",
        "ANDROID_ROOT=/system",
        "ANDROID_DATA=/data",
        "ANDROID_ART_ROOT=/apex/com.android.art",
        "ANDROID_I18N_ROOT=/apex/com.android.i18n",
        "ANDROID_TZDATA_ROOT=/apex/com.android.tzdata",
    ];
    let manager = kernel
        .spawn(
            "/system/bin/servicemanager",
            &["/system/bin/servicemanager"],
            &env,
        )
        .unwrap();
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        kernel.procs.lock().contains_key(&manager),
        "servicemanager exited: {}",
        String::from_utf8_lossy(&output.lock().unwrap())
    );
    let client = kernel
        .spawn(
            "/system/bin/service",
            &["/system/bin/service", "list"],
            &env,
        )
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while kernel.procs.lock().contains_key(&client) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let completed = !kernel.procs.lock().contains_key(&client);
    if let Some(process) = kernel.procs.lock().get(&manager).cloned() {
        kernel.exit_group(&process, indroid_kernel::task::ExitStatus::Exited(0));
    }
    let text = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
    assert!(completed, "service list timed out: {text}");
    assert!(text.contains("Found "), "service list failed: {text}");
    let _ = kernel.live_threads.load(Ordering::Relaxed);
}
