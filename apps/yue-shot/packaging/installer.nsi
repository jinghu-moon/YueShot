; Set the compression algorithm.
!if "{{compression}}" == ""
  SetCompressor /SOLID lzma
!else
  SetCompressor /SOLID "{{compression}}"
!endif

Unicode true
; Native NSIS dialogs scale fonts and controls at the system DPI.
; Do not opt into per-monitor DPI without WM_DPICHANGED layout handling.
ManifestDPIAware true

!include MUI2.nsh
!include nsDialogs.nsh
!include FileFunc.nsh
!include x64.nsh
!include "FileAssociation.nsh"

!define MANUFACTURER "{{manufacturer}}"
!define PRODUCTNAME "{{product_name}}"
!define VERSION "{{version}}"
!define VERSIONWITHBUILD "{{version_with_build}}"
!define SHORTDESCRIPTION "{{short_description}}"
!define INSTALLMODE "{{install_mode}}"
!define LICENSE "{{license}}"
!define INSTALLERICON "{{installer_icon}}"
!define SIDEBARIMAGE "{{sidebar_image}}"
!define HEADERIMAGE "{{header_image}}"
!define MAINBINARYNAME "{{main_binary_name}}"
!define MAINBINARYSRCPATH "{{main_binary_path}}"
!define IDENTIFIER "{{identifier}}"
!define COPYRIGHT "{{copyright}}"
!define OUTFILE "{{out_file}}"
!define ARCH "{{arch}}"
!define PLUGINSPATH "{{additional_plugins_path}}"
!define ALLOWDOWNGRADES "{{allow_downgrades}}"
!define DISPLAYLANGUAGESELECTOR "{{display_language_selector}}"
!define YUESHOT_INSTALLATION_ID "__YUESHOT_INSTALLATION_ID__"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${YUESHOT_INSTALLATION_ID}"
!define MANUPRODUCTKEY "Software\${MANUFACTURER}\${YUESHOT_INSTALLATION_ID}"
!define UNINSTALLERSIGNCOMMAND "{{uninstaller_sign_cmd}}"
!define YUESHOT_INSTALL_RELATIVE "__YUESHOT_INSTALL_RELATIVE__"
!define ESTIMATEDSIZE "{{estimated_size}}"

Name "${PRODUCTNAME}"
BrandingText "Yue"
ShowInstDetails hide
AutoCloseWindow true
ShowUninstDetails hide
OutFile "${OUTFILE}"

VIProductVersion "${VERSIONWITHBUILD}"
VIAddVersionKey "ProductName" "${PRODUCTNAME}"
VIAddVersionKey "FileDescription" "${SHORTDESCRIPTION}"
VIAddVersionKey "LegalCopyright" "${COPYRIGHT}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

; Plugins path, currently exists for linux only
!if "${PLUGINSPATH}" != ""
    !addplugindir "${PLUGINSPATH}"
!endif

!if "${UNINSTALLERSIGNCOMMAND}" != ""
  !uninstfinalize '${UNINSTALLERSIGNCOMMAND}'
!endif

; Handle install mode, `perUser`, `perMachine` or `both`
!if "${INSTALLMODE}" == "perMachine"
  RequestExecutionLevel highest
!endif

!if "${INSTALLMODE}" == "currentUser"
  RequestExecutionLevel user
!endif

!if "${INSTALLMODE}" == "both"
  !define MULTIUSER_MUI
  !define MULTIUSER_INSTALLMODE_INSTDIR "${PRODUCTNAME}"
  !define MULTIUSER_INSTALLMODE_COMMANDLINE
  !if "${ARCH}" == "x64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !else if "${ARCH}" == "arm64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !endif
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_KEY "${UNINSTKEY}"
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_VALUENAME "CurrentUser"
  !define MULTIUSER_INSTALLMODEPAGE_SHOWUSERNAME
  !define MULTIUSER_INSTALLMODE_FUNCTION RestorePreviousInstallLocation
  !define MULTIUSER_EXECUTIONLEVEL Highest
  !include MultiUser.nsh
!endif

; installer icon
!if "${INSTALLERICON}" != ""
  !define MUI_ICON "${INSTALLERICON}"
  !define MUI_UNICON "${INSTALLERICON}"
!endif

; The product uses a native, single options page, not the generic wizard.
; No font, bitmap, WebView, or GPUI runtime is needed to display setup.
Var PassiveMode
Var AppStartMenuFolder
Var SetupDialog
Var DirectoryInput
Var DesktopCheckbox
Var StartMenuCheckbox
Var DesktopSelected
Var StartMenuSelected
Var RunCheckbox
Var PageIcon
Var PageIconHandle
Var HeadingFont

; Share the dialog body font with the MUI header instead of its default bold font.
!define MUI_CUSTOMFUNCTION_GUIINIT InitSetupTypography
!define MUI_CUSTOMFUNCTION_UNGUIINIT un.InitSetupTypography
!macro InitSetupTypography
  SendMessage $HWNDPARENT ${WM_GETFONT} 0 0 $0
  GetDlgItem $1 $HWNDPARENT 1037
  SendMessage $1 ${WM_SETFONT} $0 1
!macroend
Function InitSetupTypography
  !insertmacro InitSetupTypography
FunctionEnd
Function un.InitSetupTypography
  !insertmacro InitSetupTypography
FunctionEnd

Page custom ShowInstallOptions LeaveInstallOptions
!define MUI_PAGE_HEADER_TEXT "$(SetupInstalling)"
!define MUI_PAGE_HEADER_SUBTEXT "$(SetupInstallingDetail)"
!insertmacro MUI_PAGE_INSTFILES
Page custom ShowInstallComplete LeaveInstallComplete

Function ShowInstallOptions
  Call SkipIfPassive
  !insertmacro MUI_HEADER_TEXT "$(SetupTitle)" "$(SetupVersion)"
  nsDialogs::Create 1018
  Pop $SetupDialog
  ${If} $SetupDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateIcon} 0 0 28u 28u ""
  Pop $PageIcon
  ${NSD_SetIconFromInstaller} $PageIcon $PageIconHandle
  ${NSD_CreateLabel} 38u 1u -38u 20u "$(SetupHeading)"
  Pop $0
  CreateFont $HeadingFont "$(^Font)" 13 400
  SendMessage $0 ${WM_SETFONT} $HeadingFont 1
  ${NSD_CreateLabel} 38u 24u -38u 20u "$(SetupScope)"
  Pop $0

  ${NSD_CreateLabel} 0 46u 100% 14u "$(SetupLocation)"
  Pop $0
  ${NSD_CreateText} 0 63u -58u 18u "$INSTDIR"
  Pop $DirectoryInput
  ${NSD_CreateBrowseButton} -52u 62u 52u 20u "$(SetupBrowse)"
  Pop $0
  ${NSD_OnClick} $0 BrowseInstallDirectory

  ${NSD_CreateCheckbox} 0 86u 100% 14u "$(SetupDesktop)"
  Pop $DesktopCheckbox
  ${NSD_SetState} $DesktopCheckbox $DesktopSelected
  ${NSD_CreateCheckbox} 0 104u 100% 14u "$(SetupStartMenu)"
  Pop $StartMenuCheckbox
  ${NSD_SetState} $StartMenuCheckbox $StartMenuSelected

  ${NSD_CreateLink} 0 125u 85u 14u "$(SetupLicense)"
  Pop $0
  ${NSD_OnClick} $0 ShowProductLicense
  ${NSD_CreateLabel} 95u 125u -95u 14u "$(SetupUnsigned)"
  Pop $0

  GetDlgItem $0 $HWNDPARENT 1
  SendMessage $0 ${WM_SETTEXT} 0 "STR:$(SetupInstall)"
  ReadRegStr $1 SHCTX "${UNINSTKEY}" "DisplayVersion"
  ${If} $1 == "${VERSION}"
    SendMessage $0 ${WM_SETTEXT} 0 "STR:$(SetupReinstall)"
  ${ElseIf} $1 != ""
    SendMessage $0 ${WM_SETTEXT} 0 "STR:$(SetupUpdate)"
  ${EndIf}
  GetDlgItem $0 $HWNDPARENT 3
  ShowWindow $0 ${SW_HIDE}
  nsDialogs::Show
  ${NSD_FreeIcon} $PageIconHandle
  System::Call 'gdi32::DeleteObject(p $HeadingFont)'
FunctionEnd

Function BrowseInstallDirectory
  Pop $0
  ${NSD_GetText} $DirectoryInput $1
  nsDialogs::SelectFolderDialog "$(SetupChooseFolder)" "$1"
  Pop $0
  ${If} $0 != error
    ${NSD_SetText} $DirectoryInput $0
  ${EndIf}
FunctionEnd

Function ShowProductLicense
  Pop $0
  InitPluginsDir
  File /oname=$PLUGINSDIR\YueShot-LICENSE.txt "${LICENSE}"
  ExecShell "open" "$PLUGINSDIR\YueShot-LICENSE.txt"
FunctionEnd

Function LeaveInstallOptions
  ${NSD_GetText} $DirectoryInput $0
  StrCmp $0 "" invalid_directory
  ; Reject drive-relative (D:folder) and current-drive (\folder) paths too.
  StrCpy $1 $0 2
  StrCmp $1 "\\" normalize_directory
  StrCpy $1 $0 2 1
  StrCmp $1 ":\" 0 invalid_directory
  normalize_directory:
  ; NSIS GetFullPathName requires an existing path; new install folders need Win32 normalization.
  System::Call 'kernel32::GetFullPathNameW(w r0,i ${NSIS_MAX_STRLEN},w .r1,p 0)i.r2'
  IntCmp $2 0 invalid_directory invalid_directory 0
  IntCmp $2 ${NSIS_MAX_STRLEN} invalid_directory 0 invalid_directory
  StrCmp $1 "" invalid_directory
  ${GetRoot} $1 $2
  StrCmp $1 "$2\" invalid_directory
  IfFileExists "$1\*" directory_valid 0
  IfFileExists "$1" invalid_directory directory_valid
  directory_valid:
  StrCpy $INSTDIR $1
  ${NSD_GetState} $DesktopCheckbox $DesktopSelected
  ${NSD_GetState} $StartMenuCheckbox $StartMenuSelected
  Return
  invalid_directory:
    MessageBox MB_OK|MB_ICONEXCLAMATION "$(SetupInvalidDirectory)"
    ${NSD_SetFocus} $DirectoryInput
    Abort
FunctionEnd

Function ShowInstallComplete
  Call SkipIfPassive
  !insertmacro MUI_HEADER_TEXT "$(SetupCompleteTitle)" "$(SetupCompleteDetail)"
  nsDialogs::Create 1018
  Pop $SetupDialog
  ${If} $SetupDialog == error
    Abort
  ${EndIf}
  ${NSD_CreateIcon} 0 8u 28u 28u ""
  Pop $PageIcon
  ${NSD_SetIconFromInstaller} $PageIcon $PageIconHandle
  ${NSD_CreateLabel} 38u 8u -38u 20u "$(SetupCompleteHeading)"
  Pop $0
  CreateFont $HeadingFont "$(^Font)" 13 400
  SendMessage $0 ${WM_SETFONT} $HeadingFont 1
  ${NSD_CreateLabel} 38u 34u -38u 34u "$(SetupInstalledLocation)"
  Pop $0
  ${NSD_CreateCheckbox} 0 86u 100% 14u "$(SetupLaunch)"
  Pop $RunCheckbox
  ${NSD_SetState} $RunCheckbox ${BST_CHECKED}
  ${NSD_CreateLabel} 0 113u 100% 30u "$(SetupDataRetained)"
  Pop $0
  GetDlgItem $0 $HWNDPARENT 1
  SendMessage $0 ${WM_SETTEXT} 0 "STR:$(SetupFinish)"
  GetDlgItem $0 $HWNDPARENT 3
  ShowWindow $0 ${SW_HIDE}
  nsDialogs::Show
  ${NSD_FreeIcon} $PageIconHandle
  System::Call 'gdi32::DeleteObject(p $HeadingFont)'
FunctionEnd

Function LeaveInstallComplete
  ${NSD_GetState} $RunCheckbox $0
  ${If} $0 == ${BST_CHECKED}
    Exec '"$INSTDIR\${MAINBINARYNAME}.exe"'
  ${EndIf}
FunctionEnd

; Uninstaller Pages
; 1. Confirm uninstall page
{{#if appdata_paths}}
Var DeleteAppDataCheckbox
Var DeleteAppDataCheckboxState
!define /ifndef WS_EX_LAYOUTRTL         0x00400000
!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.ConfirmShow
Function un.ConfirmShow
    FindWindow $1 "#32770" "" $HWNDPARENT ; Find inner dialog
    ; Anchor below the native directory field and use dialog units, not fixed pixels.
    System::Alloc 16
    Pop $R0
    GetDlgItem $2 $1 1000
    System::Call 'user32::GetWindowRect(p r2,p R0)'
    System::Call 'user32::MapWindowPoints(p 0,p r1,p R0,i 2)'
    System::Call '*$R0(i,i,i,i.r5)'
    System::Call 'user32::GetClientRect(p r1,p R0)'
    System::Call '*$R0(i,i,i.r4,i)'
    System::Call '*$R0(i 0,i 4,i 0,i 14)'
    System::Call 'user32::MapDialogRect(p r1,p R0)'
    System::Call '*$R0(i,i.r3,i,i.r2)'
    IntOp $3 $5 + $3
    System::Free $R0
    StrCpy $0 ${__NSD_CheckBox_EXSTYLE}
    ${If} $(^RTL) == 1
      IntOp $0 $0 | ${WS_EX_LAYOUTRTL}
    ${EndIf}
    System::Call 'user32::CreateWindowExW(i r0,w "${__NSD_CheckBox_CLASS}",w "$(deleteAppData)",i ${__NSD_CheckBox_STYLE},i 0,i r3,i r4,i r2,p r1,p 0,p 0,p 0)p.s'
    Pop $DeleteAppDataCheckbox
    SendMessage $HWNDPARENT ${WM_GETFONT} 0 0 $1
    SendMessage $DeleteAppDataCheckbox ${WM_SETFONT} $1 1
FunctionEnd
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE un.ConfirmLeave
Function un.ConfirmLeave
    SendMessage $DeleteAppDataCheckbox ${BM_GETCHECK} 0 0 $DeleteAppDataCheckboxState
FunctionEnd
{{/if}}
!insertmacro MUI_UNPAGE_CONFIRM

; 2. Uninstalling Page
!insertmacro MUI_UNPAGE_INSTFILES

;Languages
{{#each languages}}
!insertmacro MUI_LANGUAGE "{{this}}"
{{/each}}
!insertmacro MUI_RESERVEFILE_LANGDLL
SetFont /LANG=${LANG_ENGLISH} "Segoe UI" 10
SetFont /LANG=${LANG_SIMPCHINESE} "Microsoft YaHei UI" 10
{{#each language_files}}
  !include "{{this}}"
{{/each}}


; Application-owned strings use the same selected language as native NSIS pages.
LangString SetupInstalling ${LANG_ENGLISH} "Installing ${PRODUCTNAME}"
LangString SetupInstalling ${LANG_SIMPCHINESE} "正在安装 ${PRODUCTNAME}"
LangString SetupInstallingDetail ${LANG_ENGLISH} "Deploying files and creating your selected shortcuts."
LangString SetupInstallingDetail ${LANG_SIMPCHINESE} "正在部署程序并创建所选快捷方式。"
LangString SetupTitle ${LANG_ENGLISH} "Install ${PRODUCTNAME}"
LangString SetupTitle ${LANG_SIMPCHINESE} "安装 ${PRODUCTNAME}"
LangString SetupVersion ${LANG_ENGLISH} "Version ${VERSION}  ·  Windows x64"
LangString SetupVersion ${LANG_SIMPCHINESE} "版本 ${VERSION}  ·  Windows x64"
LangString SetupHeading ${LANG_ENGLISH} "Capture with YueShot."
LangString SetupHeading ${LANG_SIMPCHINESE} "让每一次截图，都得心应手。"
LangString SetupScope ${LANG_ENGLISH} "Install for your account. No administrator rights needed."
LangString SetupScope ${LANG_SIMPCHINESE} "仅为当前用户安装，无需管理员权限。"
LangString SetupLocation ${LANG_ENGLISH} "Install location"
LangString SetupLocation ${LANG_SIMPCHINESE} "安装位置"
LangString SetupBrowse ${LANG_ENGLISH} "Browse…"
LangString SetupBrowse ${LANG_SIMPCHINESE} "浏览…"
LangString SetupDesktop ${LANG_ENGLISH} "Create a desktop shortcut"
LangString SetupDesktop ${LANG_SIMPCHINESE} "创建桌面快捷方式"
LangString SetupStartMenu ${LANG_ENGLISH} "Add to the Start menu"
LangString SetupStartMenu ${LANG_SIMPCHINESE} "添加到开始菜单"
LangString SetupLicense ${LANG_ENGLISH} "Open-source license"
LangString SetupLicense ${LANG_SIMPCHINESE} "查看开源许可"
LangString SetupUnsigned ${LANG_ENGLISH} "This build is unsigned."
LangString SetupUnsigned ${LANG_SIMPCHINESE} "此版本未进行代码签名。"
LangString SetupInstall ${LANG_ENGLISH} "Install"
LangString SetupInstall ${LANG_SIMPCHINESE} "安装"
LangString SetupReinstall ${LANG_ENGLISH} "Reinstall"
LangString SetupReinstall ${LANG_SIMPCHINESE} "重新安装"
LangString SetupUpdate ${LANG_ENGLISH} "Update"
LangString SetupUpdate ${LANG_SIMPCHINESE} "更新"
LangString SetupChooseFolder ${LANG_ENGLISH} "Choose an install location"
LangString SetupChooseFolder ${LANG_SIMPCHINESE} "选择安装位置"
LangString SetupInvalidDirectory ${LANG_ENGLISH} "Choose an absolute program directory, such as D:\Apps\YueShot. Do not use a drive root or an existing file."
LangString SetupInvalidDirectory ${LANG_SIMPCHINESE} "请选择完整的程序安装目录，例如 D:\Apps\YueShot。请勿使用磁盘根目录或现有文件。"
LangString SetupCompleteTitle ${LANG_ENGLISH} "${PRODUCTNAME} is installed"
LangString SetupCompleteTitle ${LANG_SIMPCHINESE} "${PRODUCTNAME} 已安装"
LangString SetupCompleteDetail ${LANG_ENGLISH} "You are ready to capture."
LangString SetupCompleteDetail ${LANG_SIMPCHINESE} "准备就绪，可以开始截图了。"
LangString SetupCompleteHeading ${LANG_ENGLISH} "Installation complete"
LangString SetupCompleteHeading ${LANG_SIMPCHINESE} "安装完成"
LangString SetupInstalledLocation ${LANG_ENGLISH} "Installed to:$\r$\n$INSTDIR"
LangString SetupInstalledLocation ${LANG_SIMPCHINESE} "程序已安装到：$\r$\n$INSTDIR"
LangString SetupLaunch ${LANG_ENGLISH} "Launch ${PRODUCTNAME}"
LangString SetupLaunch ${LANG_SIMPCHINESE} "启动 ${PRODUCTNAME}"
LangString SetupDataRetained ${LANG_ENGLISH} "Updates and reinstalls keep your captures, history and settings.$\r$\nUninstalling also keeps this data by default."
LangString SetupDataRetained ${LANG_SIMPCHINESE} "升级和重新安装会保留截图、历史及设置。$\r$\n卸载时也会默认保留这些数据。"
LangString SetupFinish ${LANG_ENGLISH} "Finish"
LangString SetupFinish ${LANG_SIMPCHINESE} "完成"
LangString SetupAppRunning ${LANG_ENGLISH} "Exit YueShot normally from its tray menu before installing or uninstalling."
LangString SetupAppRunning ${LANG_SIMPCHINESE} "请先通过托盘正常退出 YueShot，再进行安装或卸载。"
LangString SetupInvalidVersion ${LANG_ENGLISH} "The installed version registration is invalid or missing. Repair it before installing."
LangString SetupInvalidVersion ${LANG_SIMPCHINESE} "已安装版本的登记信息缺失或无效，请修复后再安装。"
LangString SetupDowngrade ${LANG_ENGLISH} "A newer YueShot version is installed. Downgrading is not allowed."
LangString SetupDowngrade ${LANG_SIMPCHINESE} "已安装更新的 YueShot 版本，不能降级安装。"
LangString SetupStartupCleanupFailed ${LANG_ENGLISH} "Could not remove this installation's login startup. Uninstall stopped."
LangString SetupStartupCleanupFailed ${LANG_SIMPCHINESE} "无法清理此安装位置拥有的开机启动项，卸载已停止。"

; Windows user UI language, rather than locale, code page or old installer state.
; All Chinese UI variants use Simplified Chinese; other languages fall back to English.
; /LANG=en and /LANG=zh-CN are optional explicit overrides and support repeatable QA.
!macro SelectSetupLanguage
  System::Call 'kernel32::GetUserDefaultUILanguage()i.r0'
  IntOp $0 $0 & 0x3ff
  StrCpy $LANGUAGE ${LANG_ENGLISH}
  ${If} $0 == 4
    StrCpy $LANGUAGE ${LANG_SIMPCHINESE}
  ${EndIf}
  ${GetOptions} $CMDLINE "/LANG=" $0
  ${If} $0 == "en"
    StrCpy $LANGUAGE ${LANG_ENGLISH}
  ${ElseIf} $0 == "zh-CN"
    StrCpy $LANGUAGE ${LANG_SIMPCHINESE}
  ${EndIf}
!macroend

!macro SetContext
  !if "${INSTALLMODE}" == "currentUser"
    SetShellVarContext current
  !else if "${INSTALLMODE}" == "perMachine"
    SetShellVarContext all
  !endif

  ${If} ${RunningX64}
    !if "${ARCH}" == "x64"
      SetRegView 64
    !else if "${ARCH}" == "arm64"
      SetRegView 64
    !else
      SetRegView 32
    !endif
  ${EndIf}
!macroend

!macro CheckIfAppIsRunning
  ; Check before maintenance pages as well as immediately before file changes.
  ; A running capture/save owns resources and data. Never force-kill it.
  nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
  Pop $R0
  ${If} $R0 = 0
    SetErrorLevel 10
    Abort "$(SetupAppRunning)"
  ${EndIf}
!macroend

Function CheckInstalledVersion
  ; Enforce the same policy in interactive, passive and silent mode.
  ReadRegStr $R8 SHCTX "${UNINSTKEY}" "DisplayName"
  ReadRegStr $R9 SHCTX "${UNINSTKEY}" "DisplayVersion"
  StrCmp "$R8$R9" "" version_check_done

  ; This pinned plugin treats an invalid version as older than any valid one.
  ; 0.0.0-0 is the minimum valid SemVer, including prereleases. Refuse damaged
  ; registration instead of allowing it to bypass the downgrade policy.
  nsis_tauri_utils::SemverCompare $R9 "0.0.0-0"
  Pop $R8
  ${If} $R8 == -1
    SetErrorLevel 13
    Abort "$(SetupInvalidVersion)"
  ${EndIf}

  !if "${ALLOWDOWNGRADES}" == "false"
    nsis_tauri_utils::SemverCompare "${VERSION}" $R9
    Pop $R8
    ${If} $R8 == -1
      SetErrorLevel 12
      Abort "$(SetupDowngrade)"
    ${EndIf}
  !endif
  version_check_done:
FunctionEnd

Function .onInit
  ${GetOptions} $CMDLINE "/P" $PassiveMode
  IfErrors +2 0
    StrCpy $PassiveMode 1

  !insertmacro SelectSetupLanguage
  StrCpy $AppStartMenuFolder "${PRODUCTNAME}"
  StrCpy $DesktopSelected ${BST_CHECKED}
  StrCpy $StartMenuSelected ${BST_CHECKED}
  ${GetOptions} $CMDLINE "/NS" $0
  ${IfNot} ${Errors}
    StrCpy $DesktopSelected ${BST_UNCHECKED}
    StrCpy $StartMenuSelected ${BST_UNCHECKED}
  ${EndIf}

  !insertmacro SetContext
  !insertmacro CheckIfAppIsRunning
  Call CheckInstalledVersion

  !if "${DISPLAYLANGUAGESELECTOR}" == "true"
    !insertmacro MUI_LANGDLL_DISPLAY
  !endif

  ${If} $INSTDIR == ""
    ; Set default install location
    !if "${INSTALLMODE}" == "perMachine"
      ${If} ${RunningX64}
        !if "${ARCH}" == "x64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else if "${ARCH}" == "arm64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else
          StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
        !endif
      ${Else}
        StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
      ${EndIf}
    !else if "${INSTALLMODE}" == "currentUser"
      StrCpy $INSTDIR "$LOCALAPPDATA\${YUESHOT_INSTALL_RELATIVE}"
    !endif

    Call RestorePreviousInstallLocation
  ${EndIf}


  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_INIT
  !endif
FunctionEnd


{{#if preinstall_section}}
{{unescape_newlines preinstall_section}}
{{/if}}

Section Install
  !insertmacro CheckIfAppIsRunning
  Call CheckInstalledVersion
  SetOutPath $INSTDIR

  ; Copy main executable
  File "${MAINBINARYSRCPATH}"

  ; Create resources directory structure
  {{#each resources_dirs}}
    CreateDirectory "$INSTDIR\\{{this}}"
  {{/each}}

  ; Copy resources
  {{#each resources}}
    File /a "/oname={{this}}" "{{@key}}"
  {{/each}}

  ; Copy external binaries
  {{#each binaries}}
    File /a "/oname={{this}}" "{{@key}}"
  {{/each}}

   ; Create file associations
  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
       !insertmacro APP_ASSOCIATE "{{ext}}" "{{or association.name ext}}" "{{association-description association.description ext}}" "$INSTDIR\${MAINBINARYNAME}.exe,0" "Open with ${PRODUCTNAME}" "$INSTDIR\${MAINBINARYNAME}.exe $\"%1$\""
    {{/each}}
  {{/each}}

  ; Register deep links
  {{#each deep_link_protocols as |protocol| ~}}
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}" "URL Protocol" ""
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}" "" "URL:${BUNDLEID} protocol"
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}\DefaultIcon" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  {{/each}}

  ; Create uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Save $INSTDIR in registry for future installations
  WriteRegStr SHCTX "${MANUPRODUCTKEY}" "" $INSTDIR

  !if "${INSTALLMODE}" == "both"
    ; Save install mode to be selected by default for the next installation such as updating
    ; or when uninstalling
    WriteRegStr SHCTX "${UNINSTKEY}" $MultiUser.InstallMode 1
  !endif

  ; Registry information for add/remove programs
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr SHCTX "${UNINSTKEY}" "Publisher" "${MANUFACTURER}"
  WriteRegStr SHCTX "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
  WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegDWORD SHCTX "${UNINSTKEY}" "NoModify" "1"
  WriteRegDWORD SHCTX "${UNINSTKEY}" "NoRepair" "1"
  WriteRegDWORD SHCTX "${UNINSTKEY}" "EstimatedSize" "${ESTIMATEDSIZE}"

  ; The options page and /NS populate the same values for all install modes.
  ${If} $DesktopSelected == ${BST_CHECKED}
    Call CreateDesktopShortcut
  ${EndIf}
  ${If} $StartMenuSelected == ${BST_CHECKED}
    Call CreateStartMenuShortcut
  ${EndIf}

  ; Auto close this page for passive mode
  ${IfThen} $PassiveMode == 1 ${|} SetAutoClose true ${|}
SectionEnd

Function .onInstSuccess
  ; Check for `/R` flag only in silent and passive installers because
  ; GUI installer has a toggle for the user to (re)start the app
  IfSilent check_r_flag 0
  ${IfThen} $PassiveMode == 1 ${|} Goto check_r_flag ${|}
  Goto run_done
  check_r_flag:
    ${GetOptions} $CMDLINE "/R" $R0
    IfErrors run_done 0
      Exec '"$INSTDIR\${MAINBINARYNAME}.exe"'
  run_done:
FunctionEnd

Function un.onInit
  !insertmacro SelectSetupLanguage
  !insertmacro SetContext
  !insertmacro CheckIfAppIsRunning

  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_UNINIT
  !endif

FunctionEnd

Section Uninstall
  !insertmacro CheckIfAppIsRunning
  ; Remove only startup commands owned by this executable/channel. No data directory creation.
  ClearErrors
  ExecWait '$\"$INSTDIR\${MAINBINARYNAME}.exe$\" --uninstall-cleanup' $R0
  ${If} ${Errors}
    StrCpy $R0 1
  ${EndIf}
  ${If} $R0 != 0
    SetErrorLevel 11
    Abort "$(SetupStartupCleanupFailed)"
  ${EndIf}

  ; Delete the app directory and its content from disk
  ; Copy main executable
  Delete "$INSTDIR\${MAINBINARYNAME}.exe"

  ; Delete resources
  {{#each resources}}
    Delete "$INSTDIR\\{{this}}"
  {{/each}}

  ; Delete external binaries
  {{#each binaries}}
    Delete "$INSTDIR\\{{this}}"
  {{/each}}

  ; Delete app associations
  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
      !insertmacro APP_UNASSOCIATE "{{ext}}" "{{or association.name ext}}"
    {{/each}}
  {{/each}}

  ; Delete deep links
  {{#each deep_link_protocols as |protocol| ~}}
    ReadRegStr $R7 SHCTX "Software\Classes\\{{protocol}}\shell\open\command" ""
    !if $R7 == "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
      DeleteRegKey SHCTX "Software\Classes\\{{protocol}}"
    !endif
  {{/each}}

  ; Delete uninstaller
  Delete "$INSTDIR\uninstall.exe"

  {{#each resources_dirs}}
  RMDir /REBOOTOK "$INSTDIR\\{{this}}"
  {{/each}}
  RMDir "$INSTDIR"

  ; Remove start menu shortcut
  StrCpy $AppStartMenuFolder "${PRODUCTNAME}"
  Delete "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
  RMDir "$SMPROGRAMS\$AppStartMenuFolder"

  ; Remove desktop shortcuts
  Delete "$DESKTOP\${PRODUCTNAME}.lnk"

  ; Remove registry information for add/remove programs
  !if "${INSTALLMODE}" == "both"
    DeleteRegKey SHCTX "${UNINSTKEY}"
  !else if "${INSTALLMODE}" == "perMachine"
    DeleteRegKey HKLM "${UNINSTKEY}"
  !else
    DeleteRegKey HKCU "${UNINSTKEY}"
  !endif

  ; Remove only our own installation pointer; do not remove another installation's state.
  ReadRegStr $R0 HKCU "${MANUPRODUCTKEY}" ""
  ${If} $R0 == $INSTDIR
    DeleteRegValue HKCU "${MANUPRODUCTKEY}" ""
    DeleteRegValue HKCU "${MANUPRODUCTKEY}" "Installer Language"
    DeleteRegKey /ifempty HKCU "${MANUPRODUCTKEY}"
  ${EndIf}

  ; Delete app data
  {{#if appdata_paths}}
  ${If} $DeleteAppDataCheckboxState == 1
      SetShellVarContext current
      {{#each appdata_paths}}
      RmDir /r "{{unescape_dollar_sign this}}"
      {{/each}}
  ${EndIf}
  {{/if}}

  ${GetOptions} $CMDLINE "/P" $R0
  IfErrors +2 0
    SetAutoClose true
SectionEnd

Function RestorePreviousInstallLocation
  ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
  StrCmp $4 "" +2 0
    StrCpy $INSTDIR $4
FunctionEnd

Function SkipIfPassive
  ${IfThen} $PassiveMode == 1  ${|} Abort ${|}
FunctionEnd

Function CreateDesktopShortcut
  CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" ''
  ApplicationID::Set "$DESKTOP\${PRODUCTNAME}.lnk" "${IDENTIFIER}"
FunctionEnd

Function CreateStartMenuShortcut
  CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
  CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" ''
  ApplicationID::Set "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "${IDENTIFIER}"
FunctionEnd
