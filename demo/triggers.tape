Output assets/triggers.gif

Set Shell "bash"
Set FontSize 14
Set Width 900
Set Height 500
Set Padding 20
Set Theme "Catppuccin Mocha"

Type "# Trigger on ERROR — show 5 lines before, 10 after"
Enter
Type "lux -t 'ERROR' -b 5 -a 10 demo/sample.log"
Enter
Sleep 4s

Hide
Type "clear"
Enter
Show

Type "# Multiple triggers"
Enter
Type "lux -t 'ERROR' -t 'WARN' -b 3 -a 3 demo/sample.log"
Enter
Sleep 3s

Sleep 2s
