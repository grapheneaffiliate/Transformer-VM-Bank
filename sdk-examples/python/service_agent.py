"""
PSL service agent — Python reference example, dispute path.

Mirrors agent_sdk/examples/service_agent.rs. Demonstrates:
    1. Bob proposes a transfer.
    2. Alice accepts.
    3. Bob signs Execute claiming an all-zero output (lying).
    4. Alice opens a Dispute (her local re-execution disagrees).
    5. The judge agent re-executes the contract deterministically.
    6. Outcome is SlashExecutor(bob.pubkey).

No human arbiter, no oracle. The dispute outcome is a pure function
of (contract code, input).

Run:
    python sdk-examples/python/service_agent.py
"""

from psl_sdk import (
    AgentIdentity,
    PolicyEnvelope,
    InProcessBus,
    InMemoryOnChain,
    AgentSdk,
    TransferContract,
    Proposal,
    DisputeOutcome,
    resolve_dispute,
)


def main() -> None:
    bus = InProcessBus()
    chain = InMemoryOnChain()

    alice = AgentIdentity.generate("alice").derive_child("alice/recipient")
    bob = AgentIdentity.generate("bob").derive_child("bob/executor")

    policy = PolicyEnvelope(
        cap_per_window=10_000,
        window_seconds=3600,
        allowed_contracts=[TransferContract.program_hash()],
        allowed_counterparties=None,
        expiry_unix=2_000_000_000,
    )
    alice.attach_policy(parent=alice.parent(), policy=policy)
    bob.attach_policy(parent=bob.parent(), policy=policy)

    chain.credit(bob.pubkey(), 1000)

    alice_sdk = AgentSdk(identity=alice, transport=bus.endpoint("alice"), on_chain=chain)
    bob_sdk = AgentSdk(identity=bob, transport=bus.endpoint("bob"), on_chain=chain)

    proposal = Proposal(
        contract=TransferContract(),
        sender=bob.pubkey(),
        recipient=alice.pubkey(),
        amount=300,
        nonce=bob_sdk.next_nonce(),
    )
    propose_msg = bob_sdk.propose(proposal)
    accept_msg = alice_sdk.handle_propose(propose_msg)
    bob_sdk.handle_accept(accept_msg)

    print("bob signs Execute claiming all-zero output (lying)")
    malicious_execute = bob_sdk.execute_with_claimed_output(
        proposal.proposal_hash(),
        claimed_output_zeros=True,
    )

    outcome = alice_sdk.handle_execute(malicious_execute)
    assert outcome.is_disputed, "alice should detect the lie and dispute"
    print("alice opens Dispute (her local re-execution disagrees with bob's claim)")

    alice_sdk.open_dispute(proposal.proposal_hash())
    judge_outcome = resolve_dispute(
        contract=TransferContract(),
        proposal=proposal,
        executor_claimed=malicious_execute.claimed_output,
    )
    print(f"judge outcome: {judge_outcome}")
    assert judge_outcome == DisputeOutcome.SlashExecutor(bob.pubkey()), \
        f"expected SlashExecutor(bob), got {judge_outcome}"
    print(f"slash attributable to bob.pubkey = {bob.pubkey().hex()}")


if __name__ == "__main__":
    main()
