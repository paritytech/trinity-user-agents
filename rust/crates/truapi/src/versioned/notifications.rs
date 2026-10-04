//! Versioned wrappers for [`Notifications`](crate::api::Notifications) methods.

use alloc::vec::Vec;
use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostPushNotificationRequest { V1 => v01::HostPushNotificationRequest }
    pub enum HostPushNotificationResponse { V1 => v01::HostPushNotificationResponse }
    pub enum HostPushNotificationError { V1 => v01::HostPushNotificationError }
    pub enum HostPushNotificationCancelRequest { V1 => v01::HostPushNotificationCancelRequest }
    pub enum HostPushNotificationCancelResponse { V1 }
    pub enum HostPushNotificationCancelError { V1 => v01::GenericError }
    pub enum HostNotificationReceiverStatusRequest { V1 }
    pub enum HostNotificationReceiverStatusResponse { V1 => v01::HostNotificationReceiverStatus }
    pub enum HostNotificationReplaceReceiverRequest { V1 => v01::HostNotificationReplaceReceiverRequest }
    pub enum HostNotificationReplaceReceiverResponse { V1 => v01::HostNotificationReceiverStatus }
    pub enum HostNotificationDisableReceiverRequest { V1 => v01::HostNotificationDisableReceiverRequest }
    pub enum HostNotificationDisableReceiverResponse { V1 => v01::HostNotificationReceiverStatus }
    pub enum HostNotificationRecordReceiptRequest { V1 => v01::HostNotificationRecordReceiptRequest }
    pub enum HostNotificationRecordReceiptResponse { V1 => v01::HostNotificationReceiptResult }
    pub enum HostNotificationReceiverEventsRequest { V1 => v01::HostNotificationReceiverEventsRequest }
    pub enum HostNotificationReceiverEventsResponse { V1 => Vec<v01::ReceivingEvent> }
    pub enum HostNotificationAcknowledgeReceiverEventRequest { V1 => v01::HostNotificationAcknowledgeReceiverEventRequest }
    pub enum HostNotificationAcknowledgeReceiverEventResponse { V1 }
    pub enum HostNotificationReceivingError { V1 => v01::HostNotificationReceivingError }
}
