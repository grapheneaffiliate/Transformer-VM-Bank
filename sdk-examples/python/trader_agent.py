"""
PSL trader agent — Python reference example.

Mirrors agent_sdk/examples/trader_agent.rs. The Python SDK
(`psl_sdk`) is a UniFFI binding over the same Rust runtime, so
behavior is byte-identical to the Rust reference.

What this does:
    1. Construct Alice's identity (parent + child + spending policy).
    2. Spin up an in-process bus connecting Alice and Bob.
    3. Alice proposes a transfer of 250 units to Bob.
    4. Bob accepts.
    5. Alice signs and submits Execute.
    6. Both sides verify the executed state matches the proposed state.

This is the happy path. For the dispute path, see
`service_agent.py`.

Run:
    python sdk-examples/python/trader_agent.py
"""

from psl_sdk import (
    AgentIdentity,
    PolicyEnvelope,
    InProcessBus,
    InMemoryOnChain,
    AgentSdk,
    TransferContract,
    Proposal,
)


def main() -> None:
    bus = InProcessBus()
    chain = InMemoryOnChain()

    alice_parent = AgentIdentity.generate("alice-parent")
    alice = alice_parent.derive_child("alice/trader")
    policy = PolicyEnvelope(
        cap_per_window=1000,
        window_seconds=3600,
        allowed_contracts=[TransferContract.program_hash()],
        allowed_counterparties=None,
        expiry_unix=2_000_000_000,
    )
    alice.attach_policy(parent=alice_parent, policy=policy)

    bob_parent = AgentIdentity.generate("bob-parent")
    bob = bob_parent.derive_child("bob/service")
    bob.attach_policy(parent=bob_parent, policy=policy)

    chain.credit(alice.pubkey(), 1000)

    alice_sdk = AgentSdk(identity=alice, transport=bus.endpoint("alice"), on_chain=chain)
    bob_sdk = AgentSdk(identity=bob, transport=bus.endpoint("bob"), on_chain=chain)

    proposal = Proposal(
        contract=TransferContract(),
        sender=alice.pubkey(),
        recipient=bob.pubkey(),
        amount=250,
        nonce=alice_sdk.next_nonce(),
    )
    print(f"alice proposes transfer {proposal.amount} to bob")
    propose_msg = alice_sdk.propose(proposal)

    accept_msg = bob_sdk.handle_propose(propose_msg)
    print("bob accepts")

    alice_sdk.handle_accept(accept_msg)
    execute_msg = alice_sdk.execute(proposal.proposal_hash())
    print("alice executes")

    outcome = bob_sdk.handle_execute(execute_msg)
    assert outcome.is_settled, f"expected settled, got {outcome}"
    print(f"settled. alice balance = {chain.balance(alice.pubkey())}, bob balance = {chain.balance(bob.pubkey())}")


if __name__ == "__main__":
    main()
