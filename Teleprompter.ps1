# Chip Teleprompt for Windows v19
# Footer credits/links, manual mouse drag scroll, non-linear speed slider.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$cs = @'
using System;
using System.Runtime.InteropServices;
public static class NativeWin
{
    [DllImport("user32.dll")]
    public static extern bool ReleaseCapture();
    [DllImport("user32.dll")]
    public static extern IntPtr SendMessage(IntPtr hWnd, int Msg, IntPtr wParam, IntPtr lParam);
}
'@
Add-Type -TypeDefinition $cs

$WM_NCLBUTTONDOWN = 0x00A1
$HTCAPTION = 0x0002

function New-DefaultTextFont([float]$size, [System.Drawing.FontStyle]$style) {
  try { return New-Object System.Drawing.Font('Ubuntu', $size, $style) } catch { return New-Object System.Drawing.Font('Segoe UI', $size, $style) }
}

[System.Windows.Forms.Application]::EnableVisualStyles()

$form = New-Object System.Windows.Forms.Form
$form.Text = ''
$form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::None
$form.StartPosition = [System.Windows.Forms.FormStartPosition]::Manual
$form.MinimumSize = New-Object System.Drawing.Size(620,290)
$form.BackColor = [System.Drawing.Color]::FromArgb(5,6,10)
$form.TopMost = $true
$form.KeyPreview = $true

$table = New-Object System.Windows.Forms.TableLayoutPanel
$table.Dock = [System.Windows.Forms.DockStyle]::Fill
$table.BackColor = [System.Drawing.Color]::FromArgb(5,6,10)
$table.ColumnCount = 1
$table.RowCount = 2
[void]$table.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Percent, 100)))
[void]$table.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Absolute, 128)))
$form.Controls.Add($table)

$stage = New-Object System.Windows.Forms.Panel
$stage.Dock = [System.Windows.Forms.DockStyle]::Fill
$stage.BackColor = [System.Drawing.Color]::Black
$stage.Padding = New-Object System.Windows.Forms.Padding(16,8,16,12)
$stage.AutoScroll = $false
$table.Controls.Add($stage, 0, 0)

$scrollLabel = New-Object System.Windows.Forms.Label
$scrollLabel.AutoSize = $true
$scrollLabel.BackColor = [System.Drawing.Color]::Black
$scrollLabel.ForeColor = [System.Drawing.Color]::White
$scrollLabel.Font = New-DefaultTextFont 38 ([System.Drawing.FontStyle]::Regular)
$scrollLabel.Text = "Paste text with Paste button or Ctrl+V.`r`n`r`nChip Teleprompt. Smooth pixel scroll. Drag text with mouse for manual scroll. Speed slider has fine control on the left."
$stage.Controls.Add($scrollLabel)

$toolbar = New-Object System.Windows.Forms.Panel
$toolbar.Dock = [System.Windows.Forms.DockStyle]::Fill
$toolbar.BackColor = [System.Drawing.Color]::FromArgb(18,20,27)
$toolbar.Padding = New-Object System.Windows.Forms.Padding(10,8,10,8)
$table.Controls.Add($toolbar, 0, 1)

$accent = New-Object System.Windows.Forms.Panel
$accent.Dock = [System.Windows.Forms.DockStyle]::Top
$accent.Height = 2
$accent.BackColor = [System.Drawing.Color]::FromArgb(0,170,255)
$toolbar.Controls.Add($accent)

$toolbarGrid = New-Object System.Windows.Forms.TableLayoutPanel
$toolbarGrid.Dock = [System.Windows.Forms.DockStyle]::Fill
$toolbarGrid.ColumnCount = 1
$toolbarGrid.RowCount = 3
$toolbarGrid.BackColor = [System.Drawing.Color]::Transparent
$toolbarGrid.Padding = New-Object System.Windows.Forms.Padding(0,4,0,0)
[void]$toolbarGrid.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Absolute, 42)))
[void]$toolbarGrid.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Absolute, 42)))
[void]$toolbarGrid.RowStyles.Add((New-Object System.Windows.Forms.RowStyle([System.Windows.Forms.SizeType]::Absolute, 22)))
$toolbar.Controls.Add($toolbarGrid)
$toolbarGrid.BringToFront()

$rowButtons = New-Object System.Windows.Forms.FlowLayoutPanel
$rowButtons.Dock = [System.Windows.Forms.DockStyle]::Fill
$rowButtons.FlowDirection = [System.Windows.Forms.FlowDirection]::LeftToRight
$rowButtons.WrapContents = $false
$rowButtons.AutoScroll = $false
$rowButtons.BackColor = [System.Drawing.Color]::Transparent
$toolbarGrid.Controls.Add($rowButtons,0,0)

$rowSliders = New-Object System.Windows.Forms.FlowLayoutPanel
$rowSliders.Dock = [System.Windows.Forms.DockStyle]::Fill
$rowSliders.FlowDirection = [System.Windows.Forms.FlowDirection]::LeftToRight
$rowSliders.WrapContents = $false
$rowSliders.AutoScroll = $false
$rowSliders.BackColor = [System.Drawing.Color]::Transparent
$footer = New-Object System.Windows.Forms.FlowLayoutPanel
$footer.Dock = [System.Windows.Forms.DockStyle]::Fill
$footer.FlowDirection = [System.Windows.Forms.FlowDirection]::LeftToRight
$footer.WrapContents = $false
$footer.AutoScroll = $false
$footer.BackColor = [System.Drawing.Color]::Transparent
$toolbarGrid.Controls.Add($footer,0,2)

function New-FooterLabel([string]$text) {
  $l = New-Object System.Windows.Forms.Label
  $l.Text = $text
  $l.AutoSize = $true
  $l.ForeColor = [System.Drawing.Color]::FromArgb(145,155,170)
  $l.Font = New-Object System.Drawing.Font('Segoe UI', 8)
  $l.Margin = New-Object System.Windows.Forms.Padding(0,2,8,0)
  [void]$footer.Controls.Add($l)
  return $l
}
function New-FooterLink([string]$text, [string]$url) {
  $l = New-Object System.Windows.Forms.LinkLabel
  $l.Text = $text
  $l.AutoSize = $true
  $l.LinkColor = [System.Drawing.Color]::FromArgb(105,190,255)
  $l.ActiveLinkColor = [System.Drawing.Color]::White
  $l.VisitedLinkColor = [System.Drawing.Color]::FromArgb(105,190,255)
  $l.Font = New-Object System.Drawing.Font('Segoe UI', 8)
  $l.Margin = New-Object System.Windows.Forms.Padding(0,2,8,0)
  $l.Add_Click({ Start-Process $url }.GetNewClosure())
  [void]$footer.Controls.Add($l)
  return $l
}
[void](New-FooterLabel 'Author:')
[void](New-FooterLink '@chipcr' 'https://t.me/chipcr')
[void](New-FooterLabel 'Channel:')
[void](New-FooterLink '@human20' 'https://t.me/human20')
[void](New-FooterLabel 'Updates:')
[void](New-FooterLink 'github.com/evgyur/chip-teleprompt' 'https://github.com/evgyur/chip-teleprompt')

function New-TpButton([string]$caption, [int]$w, [System.Drawing.Color]$bg) {
  $b = New-Object System.Windows.Forms.Button
  $b.Text = $caption
  $b.Width = $w
  $b.Height = 34
  $b.Margin = New-Object System.Windows.Forms.Padding(0,2,7,0)
  $b.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
  $b.FlatAppearance.BorderColor = [System.Drawing.Color]::FromArgb(70,80,95)
  $b.FlatAppearance.MouseOverBackColor = [System.Drawing.Color]::FromArgb(55,70,90)
  $b.ForeColor = [System.Drawing.Color]::White
  $b.BackColor = $bg
  $b.Font = New-Object System.Drawing.Font('Segoe UI Semibold', 8.5)
  [void]$rowButtons.Controls.Add($b)
  return $b
}

function New-UiLabel([string]$t, [int]$w) {
  $l = New-Object System.Windows.Forms.Label
  $l.Text = $t
  $l.ForeColor = [System.Drawing.Color]::FromArgb(215,222,230)
  $l.Font = New-Object System.Drawing.Font('Segoe UI', 9)
  $l.Width = $w
  $l.Height = 32
  $l.TextAlign = [System.Drawing.ContentAlignment]::MiddleLeft
  $l.Margin = New-Object System.Windows.Forms.Padding(0,3,4,0)
  [void]$rowSliders.Controls.Add($l)
  return $l
}

$btnPaste = New-TpButton 'Paste' 64 ([System.Drawing.Color]::FromArgb(35,40,50))
$btnNoBlank = New-TpButton 'No blanks' 82 ([System.Drawing.Color]::FromArgb(35,40,50))
$btnFont  = New-TpButton 'Font' 58 ([System.Drawing.Color]::FromArgb(35,40,50))
$btnStart = New-TpButton 'Start' 76 ([System.Drawing.Color]::FromArgb(0,118,212))
$btnReset = New-TpButton 'Top' 52 ([System.Drawing.Color]::FromArgb(35,40,50))
$btnSnap  = New-TpButton 'Sticky top' 88 ([System.Drawing.Color]::FromArgb(35,40,50))
$btnView  = New-TpButton 'Clean' 62 ([System.Drawing.Color]::FromArgb(35,40,50))
$btnExit  = New-TpButton 'Exit' 52 ([System.Drawing.Color]::FromArgb(90,36,36))

$lblFontSize = New-UiLabel 'Font' 34
$fontSize = New-Object System.Windows.Forms.TrackBar
$fontSize.Width = 150; $fontSize.Height = 36
$fontSize.Minimum = 14; $fontSize.Maximum = 96; $fontSize.Value = 38; $fontSize.TickFrequency = 10
$fontSize.BackColor = [System.Drawing.Color]::FromArgb(18,20,27)
$fontSize.Margin = New-Object System.Windows.Forms.Padding(0,0,4,0)
[void]$rowSliders.Controls.Add($fontSize)
$lblFontValue = New-UiLabel "$($fontSize.Value) pt" 48

function Get-SpeedPx {
  # Non-linear mapping: left side has fine control, right side still reaches high speed.
  $t = [double]$speed.Value / 100.0
  return [int][Math]::Round(3 + (177 * $t * $t))
}

$lblSpeed = New-UiLabel 'Speed' 44
$speed = New-Object System.Windows.Forms.TrackBar
$speed.Width = 170; $speed.Height = 36
$speed.Minimum = 0; $speed.Maximum = 100; $speed.Value = 28; $speed.TickFrequency = 5
$speed.BackColor = [System.Drawing.Color]::FromArgb(18,20,27)
$speed.Margin = New-Object System.Windows.Forms.Padding(0,0,4,0)
[void]$rowSliders.Controls.Add($speed)
$lblValue = New-UiLabel "$((Get-SpeedPx)) px/s" 80

$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 16
$script:running = $false
$script:yFloat = 0.0
$script:lastTick = [Environment]::TickCount
$script:cleanMode = $false
$script:manualDrag = $false
$script:manualStartMouseY = 0
$script:manualStartY = 0.0

function Clamp-TextY {
  $topLimit = [double]$stage.Padding.Top
  $bottomLimit = [double]($stage.ClientSize.Height - $stage.Padding.Bottom - $scrollLabel.Height)
  if ($scrollLabel.Height -le ($stage.ClientSize.Height - $stage.Padding.Top - $stage.Padding.Bottom)) { $bottomLimit = $topLimit }
  if ($script:yFloat -gt $topLimit) { $script:yFloat = $topLimit }
  if ($script:yFloat -lt $bottomLimit) { $script:yFloat = $bottomLimit }
  $scrollLabel.Top = [int][Math]::Round($script:yFloat)
}

function Begin-ManualScroll($mouseY) {
  if ($script:running) { Set-Running $false }
  $script:manualDrag = $true
  $script:manualStartMouseY = $mouseY
  $script:manualStartY = $script:yFloat
  $stage.Cursor = [System.Windows.Forms.Cursors]::Hand
  $scrollLabel.Cursor = [System.Windows.Forms.Cursors]::Hand
}

function Move-ManualScroll($mouseY) {
  if (-not $script:manualDrag) { return }
  $dy = $mouseY - $script:manualStartMouseY
  $script:yFloat = [double]$script:manualStartY + [double]$dy
  Clamp-TextY
}

function End-ManualScroll {
  $script:manualDrag = $false
  $stage.Cursor = [System.Windows.Forms.Cursors]::SizeAll
  $scrollLabel.Cursor = [System.Windows.Forms.Cursors]::SizeAll
}

function Layout-Text {
  $maxWidth = [Math]::Max(100, $stage.ClientSize.Width - $stage.Padding.Left - $stage.Padding.Right)
  $scrollLabel.MaximumSize = New-Object System.Drawing.Size($maxWidth, 0)
  $scrollLabel.Left = $stage.Padding.Left
}
function Reset-Top { Layout-Text; $script:yFloat = [double]$stage.Padding.Top; Clamp-TextY }
function Set-Running([bool]$on) {
  $script:running = $on; $script:lastTick = [Environment]::TickCount
  if ($on) { $btnStart.Text='Pause'; $btnStart.BackColor=[System.Drawing.Color]::FromArgb(206,134,0); $timer.Start() }
  else { $btnStart.Text='Start'; $btnStart.BackColor=[System.Drawing.Color]::FromArgb(0,118,212); $timer.Stop() }
}
function Paste-Clipboard { if ([System.Windows.Forms.Clipboard]::ContainsText()) { $scrollLabel.Text = [System.Windows.Forms.Clipboard]::GetText(); Reset-Top } }
function Remove-Blank-Lines {
  $lines = $scrollLabel.Text -split "`r?`n"; $kept = New-Object System.Collections.Generic.List[string]
  foreach ($line in $lines) { if (-not [string]::IsNullOrWhiteSpace($line)) { [void]$kept.Add($line.TrimEnd()) } }
  $scrollLabel.Text = [string]::Join([Environment]::NewLine, $kept.ToArray()); Reset-Top
}
function Snap-Top {
  $wa = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
  $targetWidth = [int]($wa.Width * 0.5)
  $form.Width = [Math]::Max($targetWidth, $form.MinimumSize.Width)
  if ($form.Height -lt 310) { $form.Height = 410 }
  $targetLeft = [int]($wa.Left + (($wa.Width - $form.Width) / 2))
  $form.TopMost = $true; $form.Location = New-Object System.Drawing.Point($targetLeft, $wa.Top); Layout-Text; $form.Activate()
}
function Toggle-Clean {
  $script:cleanMode = -not $script:cleanMode
  if ($script:cleanMode) { $toolbar.Visible=$false; $table.RowStyles[1].Height=0; $stage.Padding=New-Object System.Windows.Forms.Padding(16,6,16,10) }
  else { $table.RowStyles[1].Height=128; $toolbar.Visible=$true; $stage.Padding=New-Object System.Windows.Forms.Padding(16,8,16,12) }
  Layout-Text
}
function Start-WindowDrag { [NativeWin]::ReleaseCapture() | Out-Null; [NativeWin]::SendMessage($form.Handle, $WM_NCLBUTTONDOWN, [IntPtr]$HTCAPTION, [IntPtr]::Zero) | Out-Null }

# Manual resize zones with visible resize cursors. Works even for borderless window.
$script:resizeMode = $null; $script:startMouse = $null; $script:startBounds = $null
function Begin-Resize([string]$mode) { $script:resizeMode=$mode; $script:startMouse=[System.Windows.Forms.Cursor]::Position; $script:startBounds=$form.Bounds }
function Do-Resize {
  if (-not $script:resizeMode) { return }
  $p=[System.Windows.Forms.Cursor]::Position; $dx=$p.X-$script:startMouse.X; $dy=$p.Y-$script:startMouse.Y
  $b=New-Object System.Drawing.Rectangle($script:startBounds.X,$script:startBounds.Y,$script:startBounds.Width,$script:startBounds.Height)
  if ($script:resizeMode.Contains('R')) { $b.Width=[Math]::Max($form.MinimumSize.Width,$script:startBounds.Width+$dx) }
  if ($script:resizeMode.Contains('B')) { $b.Height=[Math]::Max($form.MinimumSize.Height,$script:startBounds.Height+$dy) }
  if ($script:resizeMode.Contains('L')) { $newW=[Math]::Max($form.MinimumSize.Width,$script:startBounds.Width-$dx); $b.X=$script:startBounds.Right-$newW; $b.Width=$newW }
  if ($script:resizeMode.Contains('T')) { $newH=[Math]::Max($form.MinimumSize.Height,$script:startBounds.Height-$dy); $b.Y=$script:startBounds.Bottom-$newH; $b.Height=$newH }
  $form.Bounds=$b; Layout-Text
}
function End-Resize { $script:resizeMode=$null }
function Add-ResizeZone([string]$mode, [System.Windows.Forms.DockStyle]$dock, [System.Windows.Forms.Cursor]$cursor, [int]$thick) {
  $z=New-Object System.Windows.Forms.Panel; $z.Dock=$dock; $z.BackColor=[System.Drawing.Color]::Transparent; $z.Cursor=$cursor
  if ($dock -eq [System.Windows.Forms.DockStyle]::Left -or $dock -eq [System.Windows.Forms.DockStyle]::Right) { $z.Width=$thick } else { $z.Height=$thick }
  $z.Add_MouseDown({ if ($_.Button -eq [System.Windows.Forms.MouseButtons]::Left) { Begin-Resize $mode } }.GetNewClosure())
  $z.Add_MouseMove({ Do-Resize })
  $z.Add_MouseUp({ End-Resize })
  $form.Controls.Add($z); $z.BringToFront(); return $z
}
Add-ResizeZone 'L' ([System.Windows.Forms.DockStyle]::Left) ([System.Windows.Forms.Cursors]::SizeWE) 7 | Out-Null
Add-ResizeZone 'R' ([System.Windows.Forms.DockStyle]::Right) ([System.Windows.Forms.Cursors]::SizeWE) 7 | Out-Null
Add-ResizeZone 'T' ([System.Windows.Forms.DockStyle]::Top) ([System.Windows.Forms.Cursors]::SizeNS) 7 | Out-Null
Add-ResizeZone 'B' ([System.Windows.Forms.DockStyle]::Bottom) ([System.Windows.Forms.Cursors]::SizeNS) 7 | Out-Null

# Bottom-right visible grip.
$grip=New-Object System.Windows.Forms.Panel; $grip.Width=22; $grip.Height=22; $grip.Anchor='Bottom,Right'; $grip.Cursor=[System.Windows.Forms.Cursors]::SizeNWSE; $grip.BackColor=[System.Drawing.Color]::FromArgb(35,40,50)
$form.Controls.Add($grip); $grip.BringToFront()
$grip.Add_MouseDown({ if ($_.Button -eq [System.Windows.Forms.MouseButtons]::Left) { Begin-Resize 'RB' } })
$grip.Add_MouseMove({ Do-Resize })
$grip.Add_MouseUp({ End-Resize })
$form.Add_Resize({ $grip.Left=$form.ClientSize.Width-$grip.Width-2; $grip.Top=$form.ClientSize.Height-$grip.Height-2; Layout-Text })

$stage.Cursor = [System.Windows.Forms.Cursors]::SizeAll
$scrollLabel.Cursor = [System.Windows.Forms.Cursors]::SizeAll
$stage.Add_MouseDown({ if ($_.Button -eq [System.Windows.Forms.MouseButtons]::Left) { Begin-ManualScroll ([System.Windows.Forms.Cursor]::Position.Y) } })
$stage.Add_MouseMove({ Move-ManualScroll ([System.Windows.Forms.Cursor]::Position.Y) })
$stage.Add_MouseUp({ End-ManualScroll })
$scrollLabel.Add_MouseDown({ if ($_.Button -eq [System.Windows.Forms.MouseButtons]::Left) { Begin-ManualScroll ([System.Windows.Forms.Cursor]::Position.Y) } })
$scrollLabel.Add_MouseMove({ Move-ManualScroll ([System.Windows.Forms.Cursor]::Position.Y) })
$scrollLabel.Add_MouseUp({ End-ManualScroll })

$toolbar.Add_MouseDown({ if ($_.Button -eq [System.Windows.Forms.MouseButtons]::Left) { Start-WindowDrag } })
$rowButtons.Add_MouseDown({ if ($_.Button -eq [System.Windows.Forms.MouseButtons]::Left) { Start-WindowDrag } })
$rowSliders.Add_MouseDown({ if ($_.Button -eq [System.Windows.Forms.MouseButtons]::Left) { Start-WindowDrag } })

$btnPaste.Add_Click({ Paste-Clipboard })
$btnNoBlank.Add_Click({ Remove-Blank-Lines })
$btnFont.Add_Click({
  $fd = New-Object System.Windows.Forms.FontDialog; $fd.Font = $scrollLabel.Font; $fd.ShowColor = $true; $fd.Color = $scrollLabel.ForeColor
  if ($fd.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { $scrollLabel.Font=$fd.Font; $scrollLabel.ForeColor=$fd.Color; $fontSize.Value=[Math]::Max($fontSize.Minimum,[Math]::Min($fontSize.Maximum,[int][Math]::Round($fd.Font.Size))); $lblFontValue.Text="$($fontSize.Value) pt"; Layout-Text }
})
$btnStart.Add_Click({ Set-Running (-not $script:running) })
$btnReset.Add_Click({ Reset-Top })
$btnSnap.Add_Click({ Snap-Top })
$btnView.Add_Click({ Toggle-Clean })
$btnExit.Add_Click({ $form.Close() })
$fontSize.Add_ValueChanged({ $scrollLabel.Font = New-Object System.Drawing.Font($scrollLabel.Font.FontFamily, [float]$fontSize.Value, $scrollLabel.Font.Style); $lblFontValue.Text="$($fontSize.Value) pt"; Layout-Text })
$speed.Add_ValueChanged({ $lblValue.Text="$((Get-SpeedPx)) px/s" })
$stage.Add_Resize({ Layout-Text; if (-not $script:running) { Clamp-TextY } })
$timer.Add_Tick({
  if (-not $script:running) { return }
  $now=[Environment]::TickCount; $dt=[Math]::Max(1,$now-$script:lastTick)/1000.0; $script:lastTick=$now
  $script:yFloat -= ([double](Get-SpeedPx) * $dt); $scrollLabel.Top=[int][Math]::Round($script:yFloat)
  if (($scrollLabel.Bottom -lt 0) -and $script:running) { Set-Running $false }
})
$form.Add_KeyDown({
  param($sender,$e)
  if ($e.KeyCode -eq [System.Windows.Forms.Keys]::Space) { Set-Running (-not $script:running); $e.Handled=$true }
  elseif ($e.KeyCode -eq [System.Windows.Forms.Keys]::F11) { Toggle-Clean; $e.Handled=$true }
  elseif ($e.Control -and $e.KeyCode -eq [System.Windows.Forms.Keys]::V) { Paste-Clipboard; $e.Handled=$true }
  elseif ($e.Control -and $e.KeyCode -eq [System.Windows.Forms.Keys]::T) { Snap-Top; $e.Handled=$true }
  elseif (($e.KeyCode -eq [System.Windows.Forms.Keys]::Escape) -and $script:cleanMode) { Toggle-Clean; $e.Handled=$true }
})
$form.Add_Shown({ Snap-Top; Reset-Top; $grip.Left=$form.ClientSize.Width-$grip.Width-2; $grip.Top=$form.ClientSize.Height-$grip.Height-2; $stage.Focus() })
[void][System.Windows.Forms.Application]::Run($form)
