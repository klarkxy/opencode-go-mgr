!macro NSIS_HOOK_PREUNINSTALL
  ; Silent upgrades/uninstalls preserve data. Interactive removal lets the user choose.
  IfSilent skip_cleanup
  MessageBox MB_YESNO|MB_ICONQUESTION "是否同时删除数据目录 $PROFILE\.ocg-mgr？" IDNO skip_cleanup
  RMDir /r "$PROFILE\.ocg-mgr"
skip_cleanup:
!macroend
