# ==========================================
# Pinner CLI Demo
# Output: demo.gif
# ==========================================

# Optimize FPS and Dimensions
Set Framerate 15
Set Width 950
Set Height 550
Set Padding 12

# Output both a GIF for standard fallbacks and a WebP for lightweight viewing
Output demo.gif
#Output demo.webp

# Visuals & Terminal Styling
Set FontSize 15
Set Width 1200
Set Height 720
Set BorderRadius 10
Set Padding 20
Set Theme "Catppuccin Mocha"
Set LetterSpacing 1

# Base typing speed
Set TypingSpeed 60ms

# ------------------------------------------
# 1. Toolchain Readiness Check
# ------------------------------------------
Type "pinner toolchain status"
Sleep 400ms
Enter
Sleep 2s

# ------------------------------------------
# 2. Fast Project Audit
# ------------------------------------------
Type "pinner audit"
Sleep 400ms
Enter
Sleep 2.5s

# ------------------------------------------
# 3. Interactive Pin Walkthrough (TUI)
# ------------------------------------------
Type "pinner pin --walkthrough"
Sleep 400ms
Enter
Sleep 1.5s

# Navigate down through the list
Down@300ms 2
Sleep 500ms

# Toggle selected dependency
Space
Sleep 800ms

# Move down and toggle another
Down
Space
Sleep 800ms

# Confirm selection
Enter
Sleep 2s

# ------------------------------------------
# 4. Dry-Run Upgrade Preview
# ------------------------------------------
Type "pinner upgrade --dry-run"
Sleep 400ms
Enter
Sleep 2.5s

# ------------------------------------------
# 5. Interactive Upgrade Walkthrough & Exit
# ------------------------------------------
Type "pinner upgrade --walkthrough"
Sleep 400ms
Enter
Sleep 1.5s

# Highlight options
Down 2
Sleep 800ms

# Exit TUI without applying changes
Escape
Sleep 1s

# Clear screen for a clean final pause
Ctrl+L
Type "pinner check --format json"
Sleep 400ms
Enter
Sleep 3s