!macro NSIS_HOOK_PREINSTALL
  ; Product-name changes must not move an updater-managed installation.
  ${If} $UpdateMode = 1
    ReadRegStr $0 SHCTX "${MANUPRODUCTKEY}" ""
    ${If} $0 == ""
      ReadRegStr $0 SHCTX "${MANUKEY}\OCG Manager" ""
      ${If} $0 != ""
      ${AndIf} ${FileExists} "$0\${MAINBINARYNAME}.exe"
      ${AndIf} ${FileExists} "$0\uninstall.exe"
        StrCpy $INSTDIR $0
        SetOutPath $INSTDIR
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Retire old registration only when this install replaced its executable.
  ReadRegStr $0 SHCTX "${MANUKEY}\OCG Manager" ""
  ${If} $0 == $INSTDIR
    DeleteRegKey SHCTX "${MANUKEY}\OCG Manager"
    DeleteRegKey SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\OCG Manager"
    !insertmacro IsShortcutTarget "$SMPROGRAMS\OCG Manager.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      ${IfNot} ${FileExists} "$SMPROGRAMS\${PRODUCTNAME}.lnk"
        Rename "$SMPROGRAMS\OCG Manager.lnk" "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      ${Else}
        !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
        Pop $0
        ${If} $0 = 1
          Delete "$SMPROGRAMS\OCG Manager.lnk"
        ${EndIf}
      ${EndIf}
    ${EndIf}
    !insertmacro IsShortcutTarget "$DESKTOP\OCG Manager.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      ${IfNot} ${FileExists} "$DESKTOP\${PRODUCTNAME}.lnk"
        Rename "$DESKTOP\OCG Manager.lnk" "$DESKTOP\${PRODUCTNAME}.lnk"
      ${Else}
        !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
        Pop $0
        ${If} $0 = 1
          Delete "$DESKTOP\OCG Manager.lnk"
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Silent upgrades/uninstalls preserve data. Interactive removal lets the user choose.
  IfSilent skip_cleanup
  MessageBox MB_YESNO|MB_ICONQUESTION "是否同时删除数据目录 $PROFILE\.ocg-mgr？" IDNO skip_cleanup
  RMDir /r "$PROFILE\.ocg-mgr"
skip_cleanup:
!macroend
