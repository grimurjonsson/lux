Output assets/profiles.gif

Set Shell "bash"
Set FontSize 14
Set Width 900
Set Height 500
Set Padding 20
Set Theme "Catppuccin Mocha"

Type "# List available profiles"
Enter
Type "lux --list-profiles"
Enter
Sleep 2s

Hide
Type "clear"
Enter
Show

Type "# Use the logs profile"
Enter
Type "lux -p logs demo/sample.log"
Enter
Sleep 3s

Sleep 2s
