# SPDX-License-Identifier: GPL-3.0-or-later
#
# The photo upscalers' models (docs/photo-upscaling.md) from their authors' PyTorch weights to Core ML, for afp-upscale
# (macos/upscale.swift): an ML Program with one fixed input tile, 1x3xTxT RGB 0..1 in, 1x3x4Tx4T out, computed in
# float16. Then zipped for the download with the weights' license beside it (<name>.mlpackage.LICENSE: BSD-3 and
# Apache-2 both want their text with a copy), its SHA-256 and size printed for vs-gyroflow/src/upscale.rs's MODELS.
#
#   python convert.py <weights.pth> <out.mlpackage> <tile> <LICENSE of the weights' project>
#
# The weights: RealESRGAN_x4plus.pth (github.com/xinntao/Real-ESRGAN, releases v0.1.0; tile 256) and
# 003_realSR_BSRGAN_DFOWMFC_s64w8_SwinIR-L_x4_GAN.pth (github.com/JingyunLiang/SwinIR, releases v0.0; tile 192, a
# multiple of its window, 8). Needs torch (2.6 was used), coremltools (9.0) and spandrel (with einops, safetensors),
# which reads either file into its network. The model isn't loaded after the conversion: the Neural Engine's compiler
# hadn't finished SwinIR-L after ten minutes, and afp-upscale runs on the GPU.
import hashlib
import os
import sys
import time
import zipfile

# coremltools imports TensorFlow when it finds it; a broken one (a NumPy 1 build under NumPy 2) is left out
sys.modules.setdefault("tensorflow", None)
import numpy as np
import torch
import coremltools as ct
from coremltools.converters.mil import Builder as mb
from coremltools.converters.mil.frontend.torch import ops as torch_ops
from spandrel import ModelLoader

# coremltools 9.0 folds int() of a one-element constant with int(array), which NumPy 2 refuses (SwinIR's window
# padding has them): the element itself
_cast = torch_ops._cast
def _cast_one_element(context, node, dtype, dtype_name):
    x = torch_ops._get_inputs(context, node, expected=1)[0]
    if x.can_be_folded_to_const() and np.asarray(x.val).size == 1 and not isinstance(x.val, dtype):
        context.add(mb.const(val=dtype(np.asarray(x.val).reshape(-1)[0]), name=node.name), node.name)
    else:
        _cast(context, node, dtype, dtype_name)
torch_ops._cast = _cast_one_element

src, dst, tile, license_file = sys.argv[1], sys.argv[2].rstrip("/"), int(sys.argv[3]), sys.argv[4]
m = ModelLoader().load_from_file(src)
assert m.scale == 4, "x4 models only"
net = m.model.eval().float()
x = torch.rand(1, 3, tile, tile)
with torch.no_grad():
    traced = torch.jit.trace(net, x, check_trace=False)
t0 = time.time()
ml = ct.convert(traced, inputs=[ct.TensorType(name="input", shape=x.shape)], outputs=[ct.TensorType(name="output")],
                convert_to="mlprogram", minimum_deployment_target=ct.target.macOS13,
                compute_precision=ct.precision.FLOAT16, skip_model_load=True)
ml.short_description = f"{m.architecture.name} x4, a {tile}x{tile} RGB tile (0..1) in"
ml.save(dst)
print(f"{m.architecture.name}: converted in {time.time() - t0:.0f} s")
zip_path = dst + ".zip"
name = os.path.basename(dst)
with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
    for root, _, files in sorted(os.walk(dst)):
        for f in sorted(files):
            full = os.path.join(root, f)
            z.write(full, os.path.join(name, os.path.relpath(full, dst)))
    z.write(license_file, name + ".LICENSE")
digest = hashlib.sha256(open(zip_path, "rb").read()).hexdigest()
print(f"{os.path.basename(zip_path)}: {os.path.getsize(zip_path)} bytes, sha256 {digest}")
