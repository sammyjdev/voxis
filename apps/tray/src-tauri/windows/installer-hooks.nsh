; Tauri NSIS hooks for the Windows installer.
; Both editions install into the same folder under one identifier, and an
; uninstaller only removes the files its own installer wrote. So switching
; from the CUDA edition to Vulkan would keep 530 MB of cuBLAS: drop it before
; copying, and let the CUDA installer write it back.
!macro NSIS_HOOK_PREINSTALL
  Delete "$INSTDIR\cublas64_13.dll"
  Delete "$INSTDIR\cublasLt64_13.dll"
!macroend
