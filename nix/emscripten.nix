{
  stdenv,
  fetchurl,
  nodejs_24,
}:

stdenv.mkDerivation {
  pname = "emscripten";
  version = "6.0.9";

  src = fetchurl {
    url = "https://storage.googleapis.com/webassembly/emscripten-releases-builds/mac/f04ea239d533260dd1db760dd2d668d5f9a88d6b/wasm-binaries.tar.xz";
    hash = "sha256-TQaaIfBSeunm3szLEpM7CmigMU1mJT3uL5tKXJxBhhM=";
  };

  dontUnpack = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out/share
    tar -xf $src -C $out --strip-components=1
    mv $out/emscripten $out/share/emscripten
    cat > $out/share/emscripten/.emscripten <<EOF
    EMSCRIPTEN_ROOT = '$out/share/emscripten'
    LLVM_ROOT = '$out/bin'
    NODE_JS = '${nodejs_24}/bin/node'
    JS_ENGINES = [NODE_JS]
    BINARYEN_ROOT = '$out'
    EOF
    runHook postInstall
  '';
}
