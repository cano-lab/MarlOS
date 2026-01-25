#!/usr/bin/env python3
import sys
import traceback
import time

log_file = "X:/ARCH/Software/markdown_viewer/error_log.txt"

def log(msg):
    print(msg)
    with open(log_file, "a") as f:
        f.write(msg + "\n")
        f.flush()

# Clear log file
with open(log_file, "w") as f:
    f.write(f"=== App Start: {time.strftime('%Y-%m-%d %H:%M:%S')} ===\n\n")

# Global exception hook to catch ALL errors including runtime crashes
def exception_hook(exc_type, exc_value, exc_tb):
    error_msg = f"\n{'='*50}\n"
    error_msg += f"RUNTIME CRASH: {time.strftime('%Y-%m-%d %H:%M:%S')}\n"
    error_msg += f"{'='*50}\n"
    error_msg += f"Type: {exc_type.__name__}\n"
    error_msg += f"Error: {exc_value}\n"
    error_msg += f"{'='*50}\n"
    error_msg += "".join(traceback.format_exception(exc_type, exc_value, exc_tb))
    log(error_msg)
    print("\n\nCRASH LOGGED TO error_log.txt")

sys.excepthook = exception_hook

try:
    log("Starting app...")
    log(f"Python: {sys.version}")
    sys.path.insert(0, ".")

    log("\n1. Importing PyQt6...")
    from PyQt6.QtWidgets import QApplication
    log("   OK")

    log("2. Importing viewer module...")
    from viewer import MarkdownEditor
    log("   OK")

    log("3. Creating QApplication...")
    app = QApplication(sys.argv)
    log("   OK")

    log("4. Creating MarkdownEditor window...")
    window = MarkdownEditor()
    log("   OK")

    log("5. Showing window...")
    window.show()
    log("   OK")

    log("\nApp running - close window to exit")
    result = app.exec()
    log(f"App exited with code: {result}")

except Exception as e:
    error_msg = f"\n{'='*50}\nERROR: {e}\n{'='*50}\n"
    error_msg += traceback.format_exc()
    log(error_msg)

    # Keep window open on error
    print("\nError occurred! Check error_log.txt")
    print("Waiting 120 seconds...")
    time.sleep(120)

log("\nDone.")
