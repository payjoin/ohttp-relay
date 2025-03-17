# ohttp-relay Changelog

## 0.0.10

### Enable opt-in Gateway reachability for BIP 77

The [BIP 77 Draft](https://github.com/bitcoin/bips/pull/1483) imagines clients reach one another
over a "mailbox" store-and-forward server through OHTTP Relays. In order for Relays to reach those
mailbox servers without being pre-defined, this release includes support for an opt-in mechanism
based on [RFC 9540](https://www.rfc-editor.org/rfc/rfc9540.html)'s Oblivious Gateway discovery
mechanism augmented with an `allowed_purposes` parameter that may specify the BIP 77 mailbox as a
specific service.

This was activated by implementing probing functionality that caches `allowed_purposes` responses
to prevent this Relay from being party to denial of service attacks where a client might spam
requests to Gateways that do not support an allowed purpose.

- RFC 9540 was implemented in [#47](https://github.com/payjoin/ohttp-relay/pull/47)
- RFC 9458 behavior was corrected in [#46](https://github.com/payjoin/ohttp-relay/pull/46)
- Internal abstractions and ergonomics were improved in [#50](https://github.com/payjoin/ohttp-relay/pull/50), [#57](https://github.com/payjoin/ohttp-relay/pull/57), [#59](https://github.com/payjoin/ohttp-relay/pull/59), [#60](https://github.com/payjoin/ohttp-relay/pull/60), [#62](https://github.com/payjoin/ohttp-relay/pull/62), and [#63](https://github.com/payjoin/ohttp-relay/pull/63).
- Gateway opt-in was introduced in [#58](https://github.com/payjoin/ohttp-relay/pull/58)

### Gateway Probing and BIP77 Support
- Added gateway probing functionality with caching mechanism for improved performance [#46](https://github.com/payjoin/ohttp-relay/pull/46)
Implemented BIP77 purpose string detection in allowed purposes response #47
Added ALPN-encoded format parsing for gateway allowed purposes #50

- https://github.com/payjoin/ohttp-relay/pull/46
- https://github.com/payjoin/ohttp-relay/pull/47
- https://github.com/payjoin/ohttp-relay/pull/50
- https://github.com/payjoin/ohttp-relay/pull/57
- https://github.com/payjoin/ohttp-relay/pull/58
- https://github.com/payjoin/ohttp-relay/pull/59
- https://github.com/payjoin/ohttp-relay/pull/60
- https://github.com/payjoin/ohttp-relay/pull/62
- https://github.com/payjoin/ohttp-relay/pull/63

## 0.0.9

- Add `_test-util` feature to allow testing with `listen_tcp_on_free_port`
