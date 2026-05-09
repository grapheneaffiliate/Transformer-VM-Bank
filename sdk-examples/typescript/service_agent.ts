/**
 * PSL service agent — TypeScript reference example, dispute path.
 *
 * Mirrors agent_sdk/examples/service_agent.rs. Bob lies about the
 * Execute output; Alice opens a Dispute; the judge re-executes the
 * contract deterministically; outcome is SlashExecutor(bob.pubkey).
 *
 * Run:
 *   npm run service
 */

import {
  AgentIdentity,
  PolicyEnvelope,
  InProcessBus,
  InMemoryOnChain,
  AgentSdk,
  TransferContract,
  Proposal,
  DisputeOutcome,
  resolveDispute,
} from "@psl/sdk";

async function main(): Promise<void> {
  const bus = new InProcessBus();
  const chain = new InMemoryOnChain();

  const aliceP = AgentIdentity.generate("alice");
  const alice = aliceP.deriveChild("alice/recipient");
  const bobP = AgentIdentity.generate("bob");
  const bob = bobP.deriveChild("bob/executor");

  const policy: PolicyEnvelope = {
    capPerWindow: 10_000n,
    windowSeconds: 3600,
    allowedContracts: [TransferContract.programHash()],
    allowedCounterparties: null,
    expiryUnix: 2_000_000_000n,
  };
  alice.attachPolicy(aliceP, policy);
  bob.attachPolicy(bobP, policy);

  chain.credit(bob.pubkey(), 1000n);

  const aliceSdk = new AgentSdk({ identity: alice, transport: bus.endpoint("alice"), onChain: chain });
  const bobSdk = new AgentSdk({ identity: bob, transport: bus.endpoint("bob"), onChain: chain });

  const proposal: Proposal = {
    contract: new TransferContract(),
    sender: bob.pubkey(),
    recipient: alice.pubkey(),
    amount: 300n,
    nonce: bobSdk.nextNonce(),
  };
  const proposeMsg = bobSdk.propose(proposal);
  const acceptMsg = aliceSdk.handlePropose(proposeMsg);
  bobSdk.handleAccept(acceptMsg);

  console.log("bob signs Execute claiming all-zero output (lying)");
  const maliciousExecute = bobSdk.executeWithClaimedOutput(
    proposal.proposalHash(),
    { claimedOutputZeros: true },
  );

  const outcome = aliceSdk.handleExecute(maliciousExecute);
  if (!outcome.isDisputed) throw new Error("alice should detect the lie");
  console.log("alice opens Dispute");

  aliceSdk.openDispute(proposal.proposalHash());
  const judgeOutcome = resolveDispute({
    contract: new TransferContract(),
    proposal,
    executorClaimed: maliciousExecute.claimedOutput,
  });
  console.log(`judge outcome: ${judgeOutcome.kind}`);
  if (judgeOutcome.kind !== "SlashExecutor" || judgeOutcome.pubkey !== bob.pubkey()) {
    throw new Error(`expected SlashExecutor(bob), got ${JSON.stringify(judgeOutcome)}`);
  }
  console.log(`slash attributable to bob.pubkey = ${Buffer.from(bob.pubkey()).toString("hex")}`);
}

main().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
