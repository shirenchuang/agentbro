#include <windows.h>
#include <dbghelp.h>
#include <tlhelp32.h>

#include <cstdio>
#include <cstring>
#include <cwchar>

static void print_frame(HANDLE process, DWORD64 address) {
    IMAGEHLP_MODULE64 module{};
    module.SizeOfStruct = sizeof(module);
    const BOOL has_module = SymGetModuleInfo64(process, address, &module);

    alignas(SYMBOL_INFO) char buffer[sizeof(SYMBOL_INFO) + MAX_SYM_NAME]{};
    auto* symbol = reinterpret_cast<SYMBOL_INFO*>(buffer);
    symbol->SizeOfStruct = sizeof(SYMBOL_INFO);
    symbol->MaxNameLen = MAX_SYM_NAME;
    DWORD64 displacement = 0;
    if (SymFromAddr(process, address, &displacement, symbol)) {
        std::printf("  %s!%s+0x%llx", has_module ? module.ModuleName : "?",
                    symbol->Name, displacement);
        IMAGEHLP_LINE64 source{};
        source.SizeOfStruct = sizeof(source);
        DWORD line_displacement = 0;
        if (SymGetLineFromAddr64(process, address, &line_displacement, &source) && source.FileName) {
            const char* file = std::strrchr(source.FileName, '\\');
            if (!file) file = std::strrchr(source.FileName, '/');
            std::printf(" (%s:%lu)", file ? file + 1 : source.FileName, source.LineNumber);
        }
        std::putchar('\n');
    } else if (has_module) {
        std::printf("  %s+0x%llx\n", module.ModuleName, address - module.BaseOfImage);
    } else {
        std::printf("  0x%llx\n", address);
    }
}

static bool sample_thread(HANDLE process, DWORD process_id, DWORD thread_id) {
    HANDLE thread = OpenThread(THREAD_GET_CONTEXT | THREAD_SUSPEND_RESUME
                                  | THREAD_QUERY_INFORMATION,
                              FALSE, thread_id);
    if (!thread) return false;
    if (GetProcessIdOfThread(thread) != process_id || SuspendThread(thread) == DWORD(-1)) {
        CloseHandle(thread);
        return false;
    }

    DWORD64 addresses[32]{};
    unsigned count = 0;
    CONTEXT context{};
    context.ContextFlags = CONTEXT_FULL;
    if (GetThreadContext(thread, &context)) {
        STACKFRAME64 frame{};
        frame.AddrPC.Offset = context.Rip;
        frame.AddrFrame.Offset = context.Rbp;
        frame.AddrStack.Offset = context.Rsp;
        frame.AddrPC.Mode = frame.AddrFrame.Mode = frame.AddrStack.Mode = AddrModeFlat;
        addresses[count++] = context.Rip;
        for (unsigned step = 0; step < 31; ++step) {
            const DWORD64 pc = frame.AddrPC.Offset;
            const DWORD64 sp = frame.AddrStack.Offset;
            if (!StackWalk64(IMAGE_FILE_MACHINE_AMD64, process, thread, &frame, &context,
                             nullptr, SymFunctionTableAccess64, SymGetModuleBase64, nullptr)
                || !frame.AddrPC.Offset) {
                break;
            }
            if (frame.AddrPC.Offset != pc || frame.AddrStack.Offset != sp) {
                addresses[count++] = frame.AddrPC.Offset;
            }
        }
    }
    ResumeThread(thread);
    CloseHandle(thread);
    if (!count) return false;

    std::printf("Thread %lu\n", thread_id);
    for (unsigned index = 0; index < count; ++index) {
        print_frame(process, addresses[index]);
    }
    return true;
}

int wmain(int argc, wchar_t** argv) {
    if (argc != 3) {
        std::fputs("Usage: windows-startup-stacks.exe <pid> <local-symbol-directory>\n", stderr);
        return 2;
    }
    wchar_t* rest = nullptr;
    const DWORD process_id = std::wcstoul(argv[1], &rest, 10);
    if (process_id == 0 || *rest != L'\0') return 2;

    HANDLE process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, process_id);
    if (!process) {
        std::fprintf(stderr, "OpenProcess failed: %lu\n", GetLastError());
        return 1;
    }
    SymSetOptions(SYMOPT_DEFERRED_LOADS | SYMOPT_UNDNAME | SYMOPT_LOAD_LINES | SYMOPT_NO_PROMPTS
                  | SYMOPT_IGNORE_NT_SYMPATH | SYMOPT_FAIL_CRITICAL_ERRORS);
    if (!SymInitializeW(process, argv[2], TRUE)) {
        std::fprintf(stderr, "SymInitializeW failed: %lu\n", GetLastError());
        CloseHandle(process);
        return 1;
    }

    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    unsigned sampled = 0;
    if (snapshot != INVALID_HANDLE_VALUE) {
        THREADENTRY32 thread{};
        thread.dwSize = sizeof(thread);
        if (Thread32First(snapshot, &thread)) {
            do {
                if (thread.th32OwnerProcessID == process_id
                    && sample_thread(process, process_id, thread.th32ThreadID)) {
                    ++sampled;
                }
            } while (Thread32Next(snapshot, &thread));
        }
        CloseHandle(snapshot);
    }

    SymCleanup(process);
    CloseHandle(process);
    std::printf("Sampled %u AgentBro threads\n", sampled);
    return sampled ? 0 : 1;
}
