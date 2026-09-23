import Darwin
import Foundation

/// The `PATH` the person's own terminal has, which an app opened from Finder does not.
///
/// Finder starts an app with the system's directories and nothing else, and a tool installed
/// through a version manager or an npm prefix is found only on the `PATH` a login shell builds
/// from its startup files. So the app asks one, once, the way editors on macOS do: the
/// person's shell, as a login shell and an interactive one, prints `PATH` between two markers,
/// so that whatever its startup files print is not taken for the answer.
enum LoginShell {
    /// How long the shell has to answer. Startup files that load a version manager take a
    /// second or so; past this the shell is stopped and the answer is unknown.
    static let patience: TimeInterval = 5

    /// The login shell's `PATH`, or nil when it could not be asked or did not say.
    static func path(environment: [String: String]) -> String? {
        path(environment: environment, marker: "pitboard-path-\(UUID().uuidString)") {
            shell, arguments in
            run(shell, arguments, environment: environment, within: patience)
        }
    }

    /// `path(environment:)` with the shell run by `run`, so a test can say what a shell
    /// printed without starting one.
    static func path(
        environment: [String: String],
        marker: String,
        run: (_ shell: String, _ arguments: [String]) -> String?
    ) -> String? {
        guard let shell = shell(environment: environment) else { return nil }
        return run(shell, arguments(marker: marker)).flatMap { path(in: $0, marker: marker) }
    }

    /// What the shell is asked to run. `printenv` by its full path, because a startup file
    /// can define a function by that name, and not `echo $PATH`, which fish prints as a list.
    /// All three of zsh, bash and fish take the flags and the command as written.
    static func arguments(marker: String) -> [String] {
        ["-l", "-i", "-c", "echo \(marker); /usr/bin/printenv PATH; echo \(marker)"]
    }

    /// The shell to ask: `$SHELL`, which an app opened from Finder is given, or else the one
    /// the account names.
    static func shell(environment: [String: String]) -> String? {
        if let shell = environment["SHELL"], !shell.isEmpty { return shell }
        guard let account = getpwuid(getuid()), let shell = account.pointee.pw_shell else {
            return nil
        }
        let named = String(cString: shell)
        return named.isEmpty ? nil : named
    }

    /// What stands between the two markers in a shell's output, which is all of it that is
    /// the answer. Nil when either marker is missing or nothing is between them.
    static func path(in output: String, marker: String) -> String? {
        guard let open = output.range(of: marker),
            let close = output.range(of: marker, range: open.upperBound..<output.endIndex)
        else { return nil }
        let path = output[open.upperBound..<close.lowerBound].trimmingCharacters(in: .newlines)
        return path.isEmpty ? nil : path
    }

    /// What `shell` printed, run with `arguments` in `environment`. Nil when it could not
    /// start, exited with anything but success, or had not finished within `limit`, when it
    /// is stopped along with everything it started.
    ///
    /// It gets a session of its own, so an interactive shell finds no terminal to take over
    /// and everything it starts is in one process group to stop. Its input is `/dev/null`,
    /// its errors are discarded, and none of this app's other descriptors are handed to it.
    static func run(
        _ shell: String,
        _ arguments: [String],
        environment: [String: String],
        within limit: TimeInterval
    ) -> String? {
        var ends: [Int32] = [0, 0]
        guard pipe(&ends) == 0 else { return nil }
        let (reading, writing) = (ends[0], ends[1])
        defer { close(reading) }

        var actions: posix_spawn_file_actions_t?
        posix_spawn_file_actions_init(&actions)
        defer { posix_spawn_file_actions_destroy(&actions) }
        posix_spawn_file_actions_addopen(&actions, 0, "/dev/null", O_RDONLY, 0)
        posix_spawn_file_actions_adddup2(&actions, writing, 1)
        posix_spawn_file_actions_addopen(&actions, 2, "/dev/null", O_WRONLY, 0)
        var attributes: posix_spawnattr_t?
        posix_spawnattr_init(&attributes)
        defer { posix_spawnattr_destroy(&attributes) }
        posix_spawnattr_setflags(
            &attributes, Int16(POSIX_SPAWN_SETSID | POSIX_SPAWN_CLOEXEC_DEFAULT))

        let argv = ([shell] + arguments).map { strdup($0) } + [nil]
        let envp = environment.map { strdup("\($0.key)=\($0.value)") } + [nil]
        defer { for copied in argv + envp { free(copied) } }
        var pid: pid_t = 0
        let started = posix_spawn(&pid, shell, &actions, &attributes, argv, envp)
        close(writing)
        guard started == 0 else { return nil }

        let deadline = Date().addingTimeInterval(limit)
        var output = Data()
        var ended = false
        var status: Int32 = 0
        while true {
            let reaped = waitpid(pid, &status, WNOHANG)
            if reaped == pid { break }
            if reaped < 0, errno != EINTR { return nil }
            let left = deadline.timeIntervalSinceNow
            guard left > 0 else {
                stop(pid)
                return nil
            }
            if ended {
                usleep(10_000)
            } else if wait(for: reading, upTo: min(left, 0.05)) {
                ended = !readSome(reading, into: &output)
            }
        }
        // What it printed before it exited is in the pipe already. Something its startup
        // files left running may still hold the pipe open, so this takes what is there and
        // does not wait for more.
        while !ended, deadline.timeIntervalSinceNow > 0, wait(for: reading, upTo: 0) {
            ended = !readSome(reading, into: &output)
        }
        // Exited, not killed, and with status zero.
        guard status == 0 else { return nil }
        return String(decoding: output, as: UTF8.self)
    }

    /// Whether `descriptor` has something to read, or has ended, within `seconds`.
    private static func wait(for descriptor: Int32, upTo seconds: TimeInterval) -> Bool {
        var ready = pollfd(fd: descriptor, events: Int16(POLLIN), revents: 0)
        return poll(&ready, 1, Int32(seconds * 1000)) > 0
    }

    /// Reads what `descriptor` has into `output`. False once it has ended.
    private static func readSome(_ descriptor: Int32, into output: inout Data) -> Bool {
        var buffer = [UInt8](repeating: 0, count: 4096)
        let count = read(descriptor, &buffer, buffer.count)
        if count > 0 {
            output.append(contentsOf: buffer[..<count])
            return true
        }
        return count < 0 && (errno == EINTR || errno == EAGAIN)
    }

    /// Stops the shell and everything it started, and reaps it.
    private static func stop(_ pid: pid_t) {
        // Its session is its process group. An interactive shell ignores a polite signal.
        kill(-pid, SIGKILL)
        kill(pid, SIGKILL)
        var status: Int32 = 0
        while waitpid(pid, &status, 0) < 0, errno == EINTR {}
    }
}
