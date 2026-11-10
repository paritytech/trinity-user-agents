@product-sdk @allure.label.parentSuite:authenticated @allure.label.suite:host-playground @allure.label.feature:Host_API
Feature: Host API — notifications & theme

  Verify the product-sdk Host API surfaces (notifications, theme subscription)
  via the host-playground test product.

  @allure.id:14910
  Scenario: TC-11.1.1 Immediate push notification is accepted by the host
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Notifications" tab
    When the user runs "Push Notification"
    Then the result contains "Notification"

  @allure.id:14912
  Scenario: TC-11.1.3 Cancel a notification via the Host API
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Notifications" tab
    When the user runs "Cancel Notification"
    Then the result contains "Notification"

  @allure.id:14909
  Scenario: TC-10.4.1 Product receives theme updates via the Host API
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Theme" tab
    When the user runs "Subscribe Theme"
    Then the result contains "theme"

  # @skip: the host queue cap can only be reached through FUTURE-dated schedules, and
  # the product's schedule-offset field cannot be held in CI. `fill` reports success
  # and the value reads back "" — re-filled in a poll for 30s, still "" — so every one
  # of 120+ pushes went out as "scheduled now", nothing accumulated, and the cap was
  # never approached. It is not a lost keystroke on our side: driving the real product
  # locally, including with the log re-rendering under a burst, the same fill sticks and
  # the schedule lands in the future. Reproducing the CI behaviour needs instrumentation
  # inside the product, which this suite cannot add.
  #
  # The cap itself is not left uncovered: `main/notifications/NotificationsManager.spec.ts`
  # asserts ScheduleLimitReached at HOST_QUEUE_CAPACITY. Un-skip once the product's arg
  # field can be set reliably from a test.
  @allure.id:14914 @skip
  Scenario: TC-11.1.5 Scheduling past the limit returns a schedule-limit error
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Notifications" tab
    When the user schedules push notifications past the limit
    # The product (product-sdk 0.19.1) surfaces the host's ScheduleLimitReached
    # rejection as a RangeError in its log once the queue cap is exceeded.
    Then the result contains "RangeError"

  # @skip: the host no longer renders a rate-limit toast. It arrived with #494 and
  # left with #825, the move to the Rust core, which owns rate limiting now — the
  # `feature.*.rateLimiter` copy survives in all 13 locales with no consumer left in
  # `src/`. Un-skip once the host surfaces core rate-limit rejections again, and
  # delete the dead copy if it decides not to.
  @allure.id:14915 @skip
  Scenario: TC-11.2.1 Rate-limited push notifications show a toast naming the product
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Notifications" tab
    When the user fires a burst of push notifications
    Then a rate-limit toast names the product "host-playground"
