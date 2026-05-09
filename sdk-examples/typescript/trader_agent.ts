/**
 * PSL trader agent — TypeScript reference example.
 *
 * Mirrors agent_sdk/examples/trader_agent.rs. The TypeScript SDK
 * (`@psl/sdk`) is a napi-rs native addon over the same Rust runtime,
 * so behavior is byte-identical to the Rust reference.
 *
 * Run:
 *   npm run trader   (which is `tsx trader_agent.ts`)
 */

import {
  AgentIdentity,
  PolicyEnvelope,
  InProcessBus,
  InMemoryOnChain,
  AgentSdk,
  TransferContract,
  Proposal,
} from "@psl/sdk";

async function main(): Promise<void> {
  const bus = new InProcessBus();
  const chain = new InMemoryOnChain();

  const aliceParent = AgentIdentity.generate("alice-parent");
  const alice = aliceParent.deriveChild("alice/trader");
  const policy: PolicyEnvelope = {
    capPerWindow: 1000n,
    windowSeconds: 3600,
    allowedContracts: [TransferContract.programHash()],
    allowedCounterparties: null,
    expiryUnix: 2_000_000_000n,
  };
  alice.attachPolicy(aliceParent, policy);

  const bobParent = AgentIdentity.generate("bob-parent");
  const bob = bobParent.deriveChild("bob/service");
  bob.attachPolicy(bobParent, policy);

  chain.credit(alice.pubkey(), 1000n);

  const aliceSdk = new AgentSdk({
    identity: alice,
    transport: bus.endpoint("alice"),
    onChain: chain,
  });
  const bobSdk = new AgentSdk({
    identity: bob,
    transport: bus.endpoint("bob"),
    onChain: chain,
  });

  const proposal: Proposal = {
    contract: new TransferContract(),
    sender: alice.pubkey(),
    recipient: bob.pubkey(),
    amount: 250n,
    nonce: aliceSdk.nextNonce(),
  };
  console.log(`alice proposes transfer ${proposal.amount} to bob`);
  const proposeMsg = aliceSdk.propose(proposal);

  const acceptMsg = bobSdk.handlePropose(proposeMsg);
  console.log("bob accepts");

  aliceSdk.handleAccept(acceptMsg);
  const executeMsg = aliceSdk.execute(proposal.proposalHash());
  console.log("alice executes");

  const outcome = bobSdk.handleExecute(executeMsg);
  if (!outcome.isSettled) {
    throw new Error(`expected settled, got ${JSON.stringify(outcome)}`);
  }
  console.log(
    `settled. alice balance = ${chain.balance(alice.pubkey())}, bob balance = ${chain.balance(bob.pubkey())}`,
  );
}

main().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
