import subprocess
import time

# I put the wrapper code here, carefully avoiding triple quote conflicts
wrapper_code = """
import sys

# Flush buffers immediately
sys.stdout.reconfigure(line_buffering=True)
sys.stderr.reconfigure(line_buffering=True)

def __knot_loop__():
    while True:
        try:
            line = sys.stdin.readline()
            if not line: break # EOF
            
            line = line.strip()
            if line == "EXIT":
                break
                
            if line == "EXEC":
                code_lines = []
                while True:
                    l = sys.stdin.readline()
                    if not l: break
                    if l.strip() == "END_EXEC":
                        break
                    code_lines.append(l)
                
                code = "".join(code_lines)
                try:
                    exec(code, globals())
                except Exception:
                    import traceback
                    print(traceback.format_exc(), file=sys.stderr)
                
                print("---KNOT_BOUNDARY---")
                print("---KNOT_BOUNDARY---", file=sys.stderr)
        except KeyboardInterrupt:
            break
        except Exception as e:
            print(f"Internal Error: {e}", file=sys.stderr)

if __name__ == "__main__":
    __knot_loop__()
"""

def test():
    print("Starting process...")
    proc = subprocess.Popen(
        ["python3", "-u"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=0 # Unbuffered
    )

    # Send wrapper
    print("Sending wrapper...")
    proc.stdin.write(wrapper_code + "\n")
    proc.stdin.flush()
    time.sleep(0.5)

    # Send command
    print("Sending command...")
    payload = "EXEC\nprint('HELLO WORLD')\nEND_EXEC\n"
    proc.stdin.write(payload)
    proc.stdin.flush()

    # Read output
    print("Reading output...")
    while True:
        line = proc.stdout.readline()
        if not line: break
        print(f"STDOUT: {line.strip()}")
        if "---KNOT_BOUNDARY---" in line:
            break
    
    print("Test finished successfully")
    proc.terminate()

if __name__ == "__main__":
    test()