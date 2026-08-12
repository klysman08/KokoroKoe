import fixture from "../../fixtures/contracts/session-lifecycle-v1.json"

import { sessionLifecycleFixtureSchema } from "@/contracts/session-lifecycle"

it("parses the strict persisted-session lifecycle contract", () => {
  expect(sessionLifecycleFixtureSchema.parse(fixture)).toEqual(fixture)
  expect(
    sessionLifecycleFixtureSchema.safeParse({ ...fixture, extra: true })
      .success,
  ).toBe(false)
  expect(
    sessionLifecycleFixtureSchema.safeParse({
      ...fixture,
      startRequest: {
        ...fixture.startRequest,
        acknowledgedCaptureConsent: false,
      },
    }).success,
  ).toBe(false)
  expect(
    sessionLifecycleFixtureSchema.safeParse({
      ...fixture,
      persistenceStatus: {
        ...fixture.persistenceStatus,
        snapshotSequence: 10,
      },
    }).success,
  ).toBe(false)
})
