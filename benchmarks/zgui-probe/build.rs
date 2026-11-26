//! MDK is found through an rpath to the SDK's library folder, which only the executable can carry
//! (as in `ovrley_ui`'s build script). Windows has no rpath: `ovrley_playback` copies the DLLs
//! beside the executable.

fn main() {
    if let Ok(lib) = std::env::var("DEP_MDK_LIB_DIR") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{lib}");
    }
}
