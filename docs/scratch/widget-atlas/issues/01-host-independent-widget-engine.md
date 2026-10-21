# 01 — Host-independent widget engine

**What to build:** The preview widget families can be hosted by something other than the editor preview. They resolve fonts through the font resolver interface and raster images through an image source interface, instead of the preview's concrete font and image stores. Each instance exposes its tile's bounds as a typed rectangle next to the serialized geometry. The editor preview behaves exactly as before. This is a prefactor: nothing visible changes.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] Widget construction and rendering accept any font resolver and any image source; the preview session passes its existing stores.
- [ ] Every family exposes its rendered tile's bounds (left, top, width, height in scene pixels) as a typed value; the serialized geometry the frontend reads is derived from it and unchanged.
- [ ] The native font and raster resource resolvers used by export can be passed to the widgets without conversion.
- [ ] All existing preview/native parity tests and wasm preview tests pass unchanged.
- [ ] The wasm build still compiles.
