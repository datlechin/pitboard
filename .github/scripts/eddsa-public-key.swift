// Prints the Sparkle public key for a private key read from standard input, which is the
// base64 of a 32 byte ed25519 seed, the form generate_keys -x writes and sign_update
// --ed-key-file reads.
//
// The rehearsal makes its keys with `openssl rand -base64 32` and needs the matching
// public key to put in a bundle. generate_keys would do it through the login keychain,
// which is a side effect a rehearsal should not have.
//
//   swift eddsa-public-key.swift < seed
import CryptoKit
import Foundation

let seed = readLine(strippingNewline: true) ?? ""
guard let raw = Data(base64Encoded: seed),
    let key = try? Curve25519.Signing.PrivateKey(rawRepresentation: raw)
else {
    FileHandle.standardError.write(Data("standard input is not a base64 32 byte seed\n".utf8))
    exit(1)
}
print(key.publicKey.rawRepresentation.base64EncodedString())
