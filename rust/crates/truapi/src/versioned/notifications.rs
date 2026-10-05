//! Versioned wrappers for [`Notifications`](crate::api::Notifications) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostPushNotificationRequest { V1 => v01::HostPushNotificationRequest }
    pub enum HostPushNotificationResponse { V1 => v01::HostPushNotificationResponse }
    pub enum HostPushNotificationError { V1 => v01::HostPushNotificationError }
    pub enum HostPushNotificationCancelRequest { V1 => v01::HostPushNotificationCancelRequest }
    pub enum HostPushNotificationCancelResponse { V1 }
    pub enum HostPushNotificationCancelError { V1 => v01::GenericError }
    pub enum NotificationActivationEventsRequest { V1 }
    pub enum NotificationActivationEventsResponse { V1 => v01::NotificationActivations }
    pub enum NotificationActivationEventsError { V1 => v01::GenericError }
    pub enum NotificationActivationAcknowledgeRequest { V1 => v01::NotificationActivationAcknowledgeRequest }
    pub enum NotificationActivationAcknowledgeResponse { V1 }
    pub enum NotificationActivationAcknowledgeError { V1 => v01::GenericError }
}
