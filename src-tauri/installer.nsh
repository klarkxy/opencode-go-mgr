!macro NSIS_HOOK_UNINSTALL
  ; Ask the user whether to remove the application data directory.
  MessageBox MB_YESNO|MB_ICONQUESTION "是否同时删除数据目录 $PROFILE\.ocg-mgr？" IDNO skip_cleanup
  RMDir /r "$PROFILE\.ocg-mgr"
skip_cleanup:
!macroend
