param([Parameter(Mandatory=$true)][string]$Exe, [Parameter(Mandatory=$true)][string]$Report)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class TelepromptProbe {
 public delegate bool EnumProc(IntPtr hwnd, IntPtr data);
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
 [StructLayout(LayoutKind.Sequential)] public struct IconId { public uint cbSize; public IntPtr hwnd; public uint uid; public Guid guid; }
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd,StringBuilder name,int count);
 [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr hwnd,uint cmd);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd,uint msg,IntPtr wp,IntPtr lp);
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd,out Rect rect);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
 [DllImport("shell32.dll",CharSet=CharSet.Unicode)] public static extern uint ExtractIconEx(string file,int index,IntPtr large,IntPtr small,uint count);
 [DllImport("shell32.dll")] public static extern int Shell_NotifyIconGetRect(ref IconId id,out Rect rect);
 public static bool HasIcon(IntPtr hwnd) { var id=new IconId{cbSize=(uint)Marshal.SizeOf<IconId>(),hwnd=hwnd,uid=1}; Rect r;return Shell_NotifyIconGetRect(ref id,out r)>=0; }
}
'@
$previousDpi=[TelepromptProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
$testProcess=Start-Process -FilePath $Exe -WindowStyle Hidden -PassThru
$script:targetPid=$testProcess.Id
$script:appWindow=[IntPtr]::Zero
$script:dialogWindow=[IntPtr]::Zero
$results=New-Object System.Collections.Generic.List[string]
try {
 for ($attempt=0;$attempt -lt 40 -and $script:appWindow -eq [IntPtr]::Zero;$attempt++) {
  [TelepromptProbe]::EnumWindows({param($hwnd,$data)
   $owner=[uint32]0
   [void][TelepromptProbe]::GetWindowThreadProcessId($hwnd,[ref]$owner)
   if ($owner -eq $script:targetPid) {
    $className=New-Object Text.StringBuilder 128
    [void][TelepromptProbe]::GetClassName($hwnd,$className,128)
    if ($className.ToString() -eq 'ChipTelepromptRustWindow') {$script:appWindow=$hwnd}
   }
   return $true
  },[IntPtr]::Zero) | Out-Null
  if ($script:appWindow -eq [IntPtr]::Zero) {Start-Sleep -Milliseconds 50}
 }
 if ($script:appWindow -eq [IntPtr]::Zero) {throw 'Native app window not found'}
 $rect=New-Object TelepromptProbe+Rect
 [void][TelepromptProbe]::GetClientRect($script:appWindow,[ref]$rect)
 $dpi=[TelepromptProbe]::GetDpiForWindow($script:appWindow)
 if ($rect.Right -ne (700*$dpi/96) -or $rect.Bottom -ne (320*$dpi/96)) {throw "Unexpected client size: $($rect.Right)x$($rect.Bottom) dpi=$dpi"}
 $results.Add("PASS: normal launch $($rect.Right)x$($rect.Bottom) physical pixels, DPI=$dpi")
 if (-not [TelepromptProbe]::HasIcon($script:appWindow)) {throw 'Tray icon missing after normal launch'}
 $results.Add('PASS: normal launch tray registration')
 if ([TelepromptProbe]::ExtractIconEx($Exe,-1,[IntPtr]::Zero,[IntPtr]::Zero,0) -lt 1) {throw 'Embedded icon resource missing'}
 $results.Add('PASS: embedded Windows icon resource')
 [void][TelepromptProbe]::PostMessage($script:appWindow,0x8002,[IntPtr]3,[IntPtr]::Zero)
 for ($attempt=0;$attempt -lt 60 -and $script:dialogWindow -eq [IntPtr]::Zero;$attempt++) {
  [TelepromptProbe]::EnumWindows({param($hwnd,$data)
   $owner=[uint32]0
   [void][TelepromptProbe]::GetWindowThreadProcessId($hwnd,[ref]$owner)
   if ($owner -eq $script:targetPid -and [TelepromptProbe]::GetWindow($hwnd,4) -eq $script:appWindow) {
    $className=New-Object Text.StringBuilder 128
    [void][TelepromptProbe]::GetClassName($hwnd,$className,128)
    if ($className.ToString() -eq '#32770') {$script:dialogWindow=$hwnd}
   }
   return $true
  },[IntPtr]::Zero) | Out-Null
  if ($script:dialogWindow -eq [IntPtr]::Zero) {Start-Sleep -Milliseconds 50}
 }
 if ($script:dialogWindow -eq [IntPtr]::Zero) {throw 'Native font dialog did not open'}
 $results.Add('PASS: real system font dialog opens through app command')
 [void][TelepromptProbe]::PostMessage($script:dialogWindow,0x111,[IntPtr]2,[IntPtr]::Zero)
 Start-Sleep -Milliseconds 150
 [void][TelepromptProbe]::PostMessage($script:appWindow,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
 if (-not $testProcess.WaitForExit(3000)) {throw 'Application did not exit after dialog cancellation'}
 if ($testProcess.ExitCode -ne 0) {throw "Native application failed: $($testProcess.ExitCode)"}
 $results.Add('PASS: cancel font dialog and normal exit, code0')
 if ([TelepromptProbe]::HasIcon($script:appWindow)) {throw 'Tray icon leaked after exit'}
 $results.Add('PASS: tray icon removed on exit')
 $results | Set-Content -LiteralPath $Report -Encoding UTF8
 $results
} finally {
 [void][TelepromptProbe]::SetThreadDpiAwarenessContext($previousDpi)
 if (-not $testProcess.HasExited) {
  if ($script:dialogWindow -ne [IntPtr]::Zero) {[void][TelepromptProbe]::PostMessage($script:dialogWindow,0x111,[IntPtr]2,[IntPtr]::Zero)}
  if ($script:appWindow -ne [IntPtr]::Zero) {[void][TelepromptProbe]::PostMessage($script:appWindow,0x10,[IntPtr]::Zero,[IntPtr]::Zero)}
 }
}
