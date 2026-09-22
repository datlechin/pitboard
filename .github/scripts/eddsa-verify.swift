// Verifies a Sparkle EdDSA signature against a public key, not against the private key
// that made it. `sign_update --verify` takes the private key and derives the public one
// from it, so the job that signed a feed could only ever confirm its own arithmetic; the
// key an installed copy checks against is the one in the bundle it came from, and that is
// the key this takes.
//
//   swift eddsa-verify.swift <base64 public key> <base64 signature> <file>
//
// Exit 0 when the signature is this key's over those bytes, 1 when it is not, 2 when the
// arguments cannot be read at all.
import CryptoKit
import Foundation

let arguments = CommandLine.arguments
guard arguments.count == 4 else {
    FileHandle.standardError.write(Data("usage: eddsa-verify <key> <signature> <file>\n".utf8))
    exit(2)
}
guard let rawKey = Data(base64Encoded: arguments[1]),
    let key = try? Curve25519.Signing.PublicKey(rawRepresentation: rawKey)
else {
    FileHandle.standardError.write(Data("not an EdDSA public key: \(arguments[1])\n".utf8))
    exit(2)
}
guard let signature = Data(base64Encoded: arguments[2]), !signature.isEmpty else {
    // An appcast whose enclosure carries no signature at all arrives here, which is what
    // generate_appcast writes when the bundle's key and the signing key disagree.
    FileHandle.standardError.write(Data("not a signature: \"\(arguments[2])\"\n".utf8))
    exit(2)
}
guard let body = FileManager.default.contents(atPath: arguments[3]) else {
    FileHandle.standardError.write(Data("cannot read \(arguments[3])\n".utf8))
    exit(2)
}
guard key.isValidSignature(signature, for: body) else {
    FileHandle.standardError.write(
        Data("\(arguments[3]) is not signed by \(arguments[1])\n".utf8))
    exit(1)
}
