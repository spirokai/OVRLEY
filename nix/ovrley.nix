{
  src,
  lib,
  stdenv,
  fetchurl,
  fetchCrate,
  fetchPnpmDeps,
  pnpmConfigHook,
  pnpm_10,
  nodejs_24,
  makeRustPlatform,
  rustPlatform,
  rust-bin,
  buildWasmBindgenCli,
  emscripten,
  unzip,
  pkg-config,
  makeWrapper,
  python3,
  webkitgtk_4_1,
  gtk3,
  glib,
  cairo,
  pango,
  gdk-pixbuf,
  librsvg,
  openssl,
  libappindicator-gtk3,
  wrapGAppsHook3,
  gst_all_1,
}:

let
  inherit (stdenv.hostPlatform) system isDarwin isLinux;
  platform = (import ./sources.nix).${system};
  version = (builtins.fromJSON (builtins.readFile "${src}/package.json")).version;
  emscriptenLlvmBin =
    let
      config = builtins.readFile "${emscripten}/share/emscripten/.emscripten";
      afterLlvmRoot = lib.elemAt (lib.splitString "LLVM_ROOT = '" config) 1;
    in
    lib.elemAt (lib.splitString "'" afterLlvmRoot) 0;
  pnpm = pnpm_10;
  nativeRust = rust-bin.stable."1.95.0".default;
  nativeRustPlatform = makeRustPlatform {
    cargo = nativeRust;
    rustc = nativeRust;
  };
  wasmRust = rust-bin.stable."1.98.1".default.override {
    targets = [
      "wasm32-unknown-emscripten"
      "wasm32-unknown-unknown"
    ];
  };
  wasmRustPlatform = makeRustPlatform {
    cargo = wasmRust;
    rustc = wasmRust;
  };
  skiaArchive = fetchurl {
    name = "ovrley-skia-${platform.rustTarget}.tar.gz";
    url = "https://github.com/rust-skia/skia-binaries/releases/download/0.153.3/skia-binaries-b7f043e0b1e2a850e702-${platform.rustTarget}-jpegd-jpege-pdf.tar.gz";
    hash = platform.skiaHash;
  };
  ffmpegArchive = fetchurl {
    inherit (platform.ffmpeg) url hash;
  };
  ffprobeArchive =
    if platform.ffprobe != null then fetchurl { inherit (platform.ffprobe) url hash; } else null;
  pnpmDeps = fetchPnpmDeps {
    inherit src version pnpm;
    pname = "ovrley";
    fetcherVersion = 5;
    hash = "sha256-0fkyBiSt6jAwb94+hgBMbw2TvKo1lAgmSTutzYt07Nw=";
  };
  wasmWidgets = wasmRustPlatform.buildRustPackage {
    pname = "ovrley-wasm-widgets";
    inherit version src;
    cargoRoot = "src-tauri/wasm-bindings";
    cargoHash = "sha256-+hhfl6eXcIhbuFJ8i25Onw+1vUL9o0qIrpaxjsNRBFU=";
    doCheck = false;
    nativeBuildInputs = [
      emscripten
      python3
    ];
    env = {
      CARGO_TARGET_DIR = "target/ovrley-wasm-widgets";
      CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER = "${emscripten}/bin/em++";
      CC_wasm32_unknown_emscripten = "${emscripten}/bin/emcc";
      CXX_wasm32_unknown_emscripten = "${emscripten}/bin/em++";
      AR_wasm32_unknown_emscripten = "${emscripten}/bin/emar";
      EM_CONFIG = "${emscripten}/share/emscripten/.emscripten";
      EMCC_CFLAGS = "-fwasm-exceptions -msimd128";
    };
    preConfigure = ''
      export EM_CACHE="$TMPDIR/emscripten-cache"
      cp -Lr ${emscripten}/share/emscripten/cache "$EM_CACHE"
      chmod -R u+rwX "$EM_CACHE"
      export EMSDK="$TMPDIR/emsdk"
      mkdir -p "$EMSDK/upstream/emscripten"
      for source in ${emscripten}/share/emscripten/*; do
        if [ "$(basename "$source")" != cache ]; then
          ln -s "$source" "$EMSDK/upstream/emscripten/$(basename "$source")"
        fi
      done
      ln -s "$EM_CACHE" "$EMSDK/upstream/emscripten/cache"
      ln -s ${emscriptenLlvmBin} "$EMSDK/upstream/bin"
    '';
    buildPhase = ''
      runHook preBuild
      for crate in ../ovrley-wasm-widgets-${version}-vendor/source-git-*/skia-bindings-*; do
        export SKIA_SOURCE_DIR="$(realpath "$crate/skia")"
      done
      tar -xf ${src}/vendor/skia-wasm/skia-binaries-b7fa8f2b52adb915708d8be9794ec69709be086c72b825831af4de1dd57d4af2.tar.gz -C "$TMPDIR"
      export SKIA_LIBRARY_SEARCH_PATH="$TMPDIR/skia-binaries"
      export SKIA_BUILD_DEFINES="-DSK_SUPPORT_PDF -DSK_CODEC_DECODES_JPEG -DSK_CODEC_ENCODES_JPEG"
      cargo build -j "$NIX_BUILD_CORES" --offline \
        --manifest-path src-tauri/wasm-bindings/Cargo.toml \
        --target wasm32-unknown-emscripten \
        --profile wasm \
        --features skia-safe/no-compile
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      mkdir -p $out
      cp target/ovrley-wasm-widgets/wasm32-unknown-emscripten/wasm/wasm_bindings.js $out/
      cp target/ovrley-wasm-widgets/wasm32-unknown-emscripten/wasm/wasm_bindings.wasm $out/
      runHook postInstall
    '';
  };
  bindgenSrc = fetchCrate {
    pname = "wasm-bindgen-cli";
    version = "0.2.130";
    hash = "sha256-9Og1O/6/VC3sc5hBjoIWBErmLW2UHGMsTQO+6+m/dng=";
  };
  wasmBindgen = buildWasmBindgenCli {
    version = "0.2.130";
    src = bindgenSrc;
    cargoDeps = rustPlatform.fetchCargoVendor {
      src = bindgenSrc;
      hash = "sha256-j81sUf1m5yXU4FJfGj7GV+hdqVCV8mRmNb0QIsMCjqw=";
    };
  };
  wasmMap = wasmRustPlatform.buildRustPackage {
    pname = "ovrley-wasm-map";
    inherit version src;
    cargoRoot = "src-tauri/wasm-map-bindings";
    cargoHash = "sha256-91MehB/Zd7lrOi/A1oeH+rxS8JDB1iM32571+2VEp58=";
    doCheck = false;
    nativeBuildInputs = [ wasmBindgen ];
    env.CARGO_TARGET_DIR = "target/ovrley-wasm-map";
    buildPhase = ''
      runHook preBuild
      cargo build -j "$NIX_BUILD_CORES" --offline \
        --manifest-path src-tauri/wasm-map-bindings/Cargo.toml \
        --target wasm32-unknown-unknown \
        --release
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      mkdir -p $out
      wasm-bindgen \
        target/ovrley-wasm-map/wasm32-unknown-unknown/release/wasm_map_bindings.wasm \
        --target web \
        --no-typescript \
        --out-dir $out
      runHook postInstall
    '';
  };
in
nativeRustPlatform.buildRustPackage {
  pname = "ovrley";
  inherit version src pnpmDeps;
  cargoRoot = "src-tauri";
  cargoHash = "sha256-JNB+6Wf/g0zcS/W4Z/96p6541ph2UlsFtlraTYZUysg=";
  doCheck = false;

  nativeBuildInputs = [
    nodejs_24
    pnpm
    pnpmConfigHook
    pkg-config
    makeWrapper
    python3
    unzip
  ]
  ++ lib.optionals isLinux [ wrapGAppsHook3 ];
  buildInputs = lib.optionals isLinux [
    webkitgtk_4_1
    gtk3
    glib
    cairo
    pango
    gdk-pixbuf
    librsvg
    openssl
    libappindicator-gtk3
    gst_all_1.gstreamer
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good
    gst_all_1.gst-plugins-bad
    gst_all_1.gst-libav
  ];

  env = {
    OVRLEY_WASM_PREBUILT = "1";
    VITE_OVRLEY_VERSION = version;
    TAURI_CONFIG = builtins.readFile ./tauri.conf.json;
    BINDGEN_EXTRA_CLANG_ARGS = lib.optionalString isDarwin "-I${stdenv.cc.libcxx}/include/c++/v1";
  };
  cargoBuildFlags = [
    "--features"
    "tauri/custom-protocol"
  ];
  buildAndTestSubdir = "src-tauri";

  preBuild = ''
    for crate in ../ovrley-${version}-vendor/source-git-*/skia-bindings-*; do
      export SKIA_SOURCE_DIR="$(realpath "$crate/skia")"
    done
    tar -xf ${skiaArchive} -C "$TMPDIR"
    export SKIA_LIBRARY_SEARCH_PATH="$TMPDIR/skia-binaries"
    export SKIA_BUILD_DEFINES="-DSK_SUPPORT_PDF -DSK_CODEC_DECODES_JPEG -DSK_CODEC_ENCODES_JPEG"
    mkdir -p src-tauri/target/wasm-bindings/wasm32-unknown-emscripten/wasm
    cp ${wasmWidgets}/* src-tauri/target/wasm-bindings/wasm32-unknown-emscripten/wasm/
    cp ${wasmMap}/* src-tauri/target/wasm-bindings/wasm32-unknown-emscripten/wasm/

    mkdir -p vendor/ffmpeg/bin
    ${
      if isDarwin then
        ''
          unzip -q ${ffmpegArchive} -d vendor/ffmpeg/bin
          unzip -q ${ffprobeArchive} -d vendor/ffmpeg/bin
        ''
      else
        ''
          tar -xf ${ffmpegArchive} -C "$TMPDIR"
          cp "$TMPDIR"/ffmpeg-*-linux*-gpl/bin/ffmpeg vendor/ffmpeg/bin/
          cp "$TMPDIR"/ffmpeg-*-linux*-gpl/bin/ffprobe vendor/ffmpeg/bin/
        ''
    }
    chmod +x vendor/ffmpeg/bin/ffmpeg vendor/ffmpeg/bin/ffprobe

    pnpm install --offline --frozen-lockfile --ignore-scripts
    pnpm --dir app build
    node packaging/resources.mjs
  '';

  installPhase = ''
    runHook preInstall
    ${
      if isDarwin then
        ''
          app="$out/Applications/OVRLEY.app/Contents"
          mkdir -p "$app/MacOS" "$app/Resources"
          cp target/${platform.rustTarget}/release/app "$app/MacOS/OVRLEY"
          cp ${./Info.plist} "$app/Info.plist"
          cp src-tauri/icons/icon.icns "$app/Resources/icon.icns"
          resources="$app/Resources"
          touch "$app/MacOS/.ovrley-portable"
          mkdir -p $out/bin
          makeWrapper "$app/MacOS/OVRLEY" "$out/bin/OVRLEY" \
            --set OVRLEY_FFMPEG "$app/Resources/vendor/ffmpeg/bin/ffmpeg"
        ''
      else
        ''
          mkdir -p $out/libexec/ovrley $out/bin
          cp target/${platform.rustTarget}/release/app $out/libexec/ovrley/OVRLEY-bin
          resources="$out/libexec/ovrley"
          touch "$out/libexec/ovrley/.ovrley-portable"
          makeWrapper "$out/libexec/ovrley/OVRLEY-bin" "$out/bin/OVRLEY" \
            --set OVRLEY_FFMPEG "$out/libexec/ovrley/vendor/ffmpeg/bin/ffmpeg" \
            --prefix PATH : ${lib.makeBinPath [ gst_all_1.gstreamer ]} \
            --prefix GST_PLUGIN_SYSTEM_PATH_1_0 : "$GST_PLUGIN_SYSTEM_PATH_1_0"
          install -Dm644 packaging/appimage/com.ovrley.desktop $out/share/applications/com.ovrley.desktop
          install -Dm644 src-tauri/icons/icon.png $out/share/icons/hicolor/512x512/apps/ovrley.png
        ''
    }
    cp -r fonts templates "$resources/"
    mkdir -p "$resources/map-styles"
    cp -r assets/map-styles/. "$resources/map-styles/"
    mkdir -p "$resources/vendor"
    cp -r vendor/ffmpeg "$resources/vendor/"
    chmod +x "$resources/vendor/ffmpeg/bin/ffmpeg" "$resources/vendor/ffmpeg/bin/ffprobe"
    cp src-tauri/target/packaging/THIRD_PARTY_NOTICES.txt "$resources/"
    runHook postInstall
  '';

  passthru = {
    inherit wasmWidgets wasmMap wasmBindgen;
  };

  meta = {
    description = "Customizable telemetry overlays for activity videos";
    homepage = "https://www.ovrley.cc";
    license = lib.licenses.gpl3Plus;
    mainProgram = "OVRLEY";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
  };
}
