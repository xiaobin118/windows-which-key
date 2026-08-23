!include "MUI2.nsh"

!define APP_NAME "Which-Key Windows"
!define APP_EXE "which-key-windows.exe"
!define APP_REG_KEY "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Which-Key Windows"
!define APP_VENDOR "Which-Key"

Name "${APP_NAME}"
OutFile "dist\\which-key-windows-setup.exe"
InstallDir "$PROGRAMFILES64\\Which-Key Windows"
InstallDirRegKey HKLM "${APP_REG_KEY}" "InstallLocation"
RequestExecutionLevel admin

!define MUI_ABORTWARNING
!define MUI_PAGE_WELCOME
!define MUI_PAGE_DIRECTORY
!define MUI_PAGE_INSTFILES
!define MUI_UNPAGE_CONFIRM
!define MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "MainSection" SecMain
  SetOutPath "$INSTDIR"
  File "..\\target\\release\\which-key-windows.exe"
  File "..\\README.md"
  File "..\\README.zh-CN.md"

  CreateDirectory "$SMPROGRAMS\\Which-Key Windows"
  CreateShortCut "$SMPROGRAMS\\Which-Key Windows\\Which-Key Windows.lnk" "$INSTDIR\\${APP_EXE}"
  CreateShortCut "$DESKTOP\\Which-Key Windows.lnk" "$INSTDIR\\${APP_EXE}"

  WriteUninstaller "$INSTDIR\\Uninstall.exe"

  WriteRegStr HKLM "${APP_REG_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKLM "${APP_REG_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${APP_REG_KEY}" "Publisher" "${APP_VENDOR}"
  WriteRegStr HKLM "${APP_REG_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${APP_REG_KEY}" "DisplayIcon" "$INSTDIR\\${APP_EXE}"
  WriteRegStr HKLM "${APP_REG_KEY}" "UninstallString" '"$INSTDIR\\Uninstall.exe"'
  WriteRegDWORD HKLM "${APP_REG_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${APP_REG_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$DESKTOP\\Which-Key Windows.lnk"
  Delete "$SMPROGRAMS\\Which-Key Windows\\Which-Key Windows.lnk"
  RMDir "$SMPROGRAMS\\Which-Key Windows"

  Delete "$INSTDIR\\${APP_EXE}"
  Delete "$INSTDIR\\README.md"
  Delete "$INSTDIR\\README.zh-CN.md"
  Delete "$INSTDIR\\Uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "${APP_REG_KEY}"
SectionEnd
