//! Declarative composition macros for typed domains.
//! No proc macros, no recursion and no identifier synthesis.
//! Each invocation names its access/domain types and one monomorphic body;
//! the access half wraps the hand-written JSON adapters (`QueryAccess` /
//! `MutationAccess`) instead of re-authoring their boundary logic.
//!
//! The one-author rule holds at birth: the JSON boundary — route refusal,
//! argument decoding, replay ordering and output encoding — is authored once
//! in `access`, and every declaration macro adopts that kit by delegation.
//! Only the domain struct and its typed contract impl are authored here.

#[macro_export]
macro_rules! daf_query {
    (
        $(#[$access_meta:meta])* $access_vis:vis access $access:ident,
        $(#[$domain_meta:meta])* $domain_vis:vis domain $domain:ident,
        resource = $resource:literal,
        pool = $pool:ty,
        input = $input:ty,
        output = $output:ty,
        run($pool_name:ident, $input_name:ident, $subject_name:ident)
            $body:block
    ) => {
        $(#[$domain_meta])*
        #[derive(Debug, Clone, Copy, Default)]
        $domain_vis struct $domain;

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::QueryDomain<$pool> for $domain {
            const RESOURCE: &'static str = $resource;
            type Input = $input;
            type Output = $output;

            async fn run(
                &self,
                $pool_name: Option<&$pool>,
                $input_name: Self::Input,
                $subject_name: &$crate::generic::Subject,
            ) -> ::core::result::Result<
                <Self as $crate::generic::QueryDomain<$pool>>::Output,
                $crate::DataAccessError,
            > {
                debug_assert!(!$resource.is_empty(), "macro query resources are named");
                $body
            }
        }

        $(#[$access_meta])*
        #[derive(Debug, Clone)]
        #[allow(missing_docs)]
        $access_vis struct $access {
            inner: $crate::generic::QueryAccess<$domain, $pool>,
        }

        #[allow(missing_docs)]
        impl $access {
            #[must_use]
            pub const fn new(pool: Option<$pool>) -> Self {
                Self {
                    inner: $crate::generic::QueryAccess::new($domain, pool),
                }
            }

            #[must_use]
            pub const fn pool(&self) -> Option<&$pool> {
                self.inner.pool()
            }
        }

        #[allow(missing_docs)]
        impl ::core::default::Default for $access {
            fn default() -> Self {
                Self {
                    inner: $crate::generic::QueryAccess::new($domain, None),
                }
            }
        }

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::DataAccess for $access {
            async fn query(
                &self,
                info: &$crate::generic::QueryInfo,
                subject: &$crate::generic::Subject,
            ) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                debug_assert!(
                    !info.resource.is_empty(),
                    "query requests name the resource"
                );
                self.inner.query(info, subject).await
            }

            async fn execute_mutation(
                &self,
                info: &$crate::generic::MutationInfo,
                subject: &$crate::generic::Subject,
                context: &$crate::generic::MutationContext,
            ) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                debug_assert!(
                    !info.resource.is_empty(),
                    "mutation requests name the resource"
                );
                self.inner.execute_mutation(info, subject, context).await
            }
        }
    };
}

#[macro_export]
macro_rules! daf_mutation {
    (
        $(#[$access_meta:meta])* $access_vis:vis access $access:ident,
        $(#[$domain_meta:meta])* $domain_vis:vis domain $domain:ident,
        resource = $resource:literal,
        pool = $pool:ty,
        input = $input:ty,
        output = $output:ty,
        idempotency = none,
        run($pool_name:ident, $input_name:ident, $subject_name:ident, $context_name:ident)
            $body:block
    ) => {
        $(#[$domain_meta])*
        #[derive(Debug, Clone, Copy, Default)]
        $domain_vis struct $domain;

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::MutationDomain<$pool> for $domain {
            const RESOURCE: &'static str = $resource;
            type Input = $input;
            type Output = $output;

            fn idempotency_key(&self, _subject: &$crate::generic::Subject, _input: &Self::Input) -> Option<String> {
                debug_assert!(!$resource.is_empty(), "idempotency names a resource");
                None
            }

            async fn run(
                &self,
                $pool_name: Option<&$pool>,
                $input_name: Self::Input,
                $subject_name: &$crate::generic::Subject,
                $context_name: &$crate::generic::MutationContext,
            ) -> ::core::result::Result<
                <Self as $crate::generic::MutationDomain<$pool>>::Output,
                $crate::DataAccessError,
            > {
                debug_assert!(!$resource.is_empty(), "macro mutation resources are named");
                $body
            }
        }

        $(#[$access_meta])*
        #[derive(Clone)]
        #[allow(missing_docs)]
        $access_vis struct $access {
            inner: $crate::generic::MutationAccess<$domain, $pool>,
        }

        #[allow(missing_docs)]
        impl $access {
            #[must_use]
            pub const fn new(pool: Option<$pool>) -> Self {
                Self {
                    inner: $crate::generic::MutationAccess::new($domain, pool),
                }
            }

            #[must_use]
            pub fn with_idempotency(mut self, store: ::std::sync::Arc<dyn $crate::generic::Idempotency>) -> Self {
                debug_assert!(
                    ::std::mem::size_of::<Option<::std::sync::Arc<dyn $crate::generic::Idempotency>>>() > 0,
                    "idempotency slot is representable"
                );
                self.inner = self.inner.with_idempotency(store);
                self
            }

            #[must_use]
            pub const fn pool(&self) -> Option<&$pool> {
                self.inner.pool()
            }
        }

        #[allow(missing_docs)]
        impl ::core::default::Default for $access {
            fn default() -> Self {
                Self {
                    inner: $crate::generic::MutationAccess::new($domain, None),
                }
            }
        }

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::DataAccess for $access {
            async fn query(
                &self,
                info: &$crate::generic::QueryInfo,
                subject: &$crate::generic::Subject,
            ) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                debug_assert!(
                    !info.resource.is_empty(),
                    "query requests name the resource"
                );
                self.inner.query(info, subject).await
            }

            async fn execute_mutation(
                &self,
                info: &$crate::generic::MutationInfo,
                subject: &$crate::generic::Subject,
                context: &$crate::generic::MutationContext,
            ) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                debug_assert!(
                    !info.resource.is_empty(),
                    "mutation requests name the resource"
                );
                self.inner.execute_mutation(info, subject, context).await
            }
        }
    };
    (
        $(#[$access_meta:meta])* $access_vis:vis access $access:ident,
        $(#[$domain_meta:meta])* $domain_vis:vis domain $domain:ident,
        resource = $resource:literal,
        pool = $pool:ty,
        input = $input:ty,
        output = $output:ty,
        idempotency = subject_and_input,
        run($pool_name:ident, $input_name:ident, $subject_name:ident, $context_name:ident)
            $body:block
    ) => {
        $(#[$domain_meta])*
        #[derive(Debug, Clone, Copy, Default)]
        $domain_vis struct $domain;

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::MutationDomain<$pool> for $domain {
            const RESOURCE: &'static str = $resource;
            type Input = $input;
            type Output = $output;

            fn idempotency_key(&self, $subject_name: &$crate::generic::Subject, $input_name: &Self::Input) -> Option<String> {
                debug_assert!(!$resource.is_empty(), "idempotency names a resource");
                Some($crate::generic::subject_json_key($subject_name, $input_name))
            }

            async fn run(
                &self,
                $pool_name: Option<&$pool>,
                $input_name: Self::Input,
                $subject_name: &$crate::generic::Subject,
                $context_name: &$crate::generic::MutationContext,
            ) -> ::core::result::Result<
                <Self as $crate::generic::MutationDomain<$pool>>::Output,
                $crate::DataAccessError,
            > {
                debug_assert!(!$resource.is_empty(), "macro mutation resources are named");
                $body
            }
        }

        $(#[$access_meta])*
        #[derive(Clone)]
        #[allow(missing_docs)]
        $access_vis struct $access {
            inner: $crate::generic::MutationAccess<$domain, $pool>,
        }

        #[allow(missing_docs)]
        impl $access {
            #[must_use]
            pub const fn new(pool: Option<$pool>) -> Self {
                Self {
                    inner: $crate::generic::MutationAccess::new($domain, pool),
                }
            }

            #[must_use]
            pub fn with_idempotency(mut self, store: ::std::sync::Arc<dyn $crate::generic::Idempotency>) -> Self {
                debug_assert!(
                    ::std::mem::size_of::<Option<::std::sync::Arc<dyn $crate::generic::Idempotency>>>() > 0,
                    "idempotency slot is representable"
                );
                self.inner = self.inner.with_idempotency(store);
                self
            }

            #[must_use]
            pub const fn pool(&self) -> Option<&$pool> {
                self.inner.pool()
            }
        }

        #[allow(missing_docs)]
        impl ::core::default::Default for $access {
            fn default() -> Self {
                Self {
                    inner: $crate::generic::MutationAccess::new($domain, None),
                }
            }
        }

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::DataAccess for $access {
            async fn query(
                &self,
                info: &$crate::generic::QueryInfo,
                subject: &$crate::generic::Subject,
            ) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                debug_assert!(
                    !info.resource.is_empty(),
                    "query requests name the resource"
                );
                self.inner.query(info, subject).await
            }

            async fn execute_mutation(
                &self,
                info: &$crate::generic::MutationInfo,
                subject: &$crate::generic::Subject,
                context: &$crate::generic::MutationContext,
            ) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                debug_assert!(
                    !info.resource.is_empty(),
                    "mutation requests name the resource"
                );
                self.inner.execute_mutation(info, subject, context).await
            }
        }
    };
}

#[macro_export]
macro_rules! daf_policy {
    (
        $(#[$meta:meta])* $policy_vis:vis policy struct $policy:ident;
        query { $( $query_resource:literal => $query_mode:ident ),+ $(,)? }
        $( mutation { $( $mutation_resource:literal => $mutation_mode:ident ),+ $(,)? } )?
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default)]
        $policy_vis struct $policy;

        impl $policy {
            #[must_use]
            pub fn policies() -> &'static [$crate::generic::ResourcePolicy] {
                debug_assert!(
                    ::std::mem::size_of::<Option<&'static str>>() > 0,
                    "resource identifiers are representable"
                );
                &[
                    $( $crate::generic::ResourcePolicy {
                        resource: $query_resource,
                        query: Some($crate::generic::AdmissionMode::$query_mode),
                        mutation: None,
                    }, )+
                    $( $( $crate::generic::ResourcePolicy {
                        resource: $mutation_resource,
                        query: None,
                        mutation: Some($crate::generic::AdmissionMode::$mutation_mode),
                    }, )* )?
                ]
            }
        }

        impl $crate::generic::PolicyTable for $policy {
            fn policies() -> &'static [$crate::generic::ResourcePolicy] {
                debug_assert!(
                    ::std::mem::size_of::<Option<&'static str>>() > 0,
                    "resource identifiers are representable"
                );
                $policy::policies()
            }
        }
    };
}

#[macro_export]
macro_rules! daf_rows {
    (
        $(#[$meta:meta])*
        $function_vis:vis fn $function:ident($pool_name:ident: Option<$pool:ty>) ->
            $returned:ty;
        $(row $resource:literal => $access:expr),+ $(,)?
    ) => {
        $(#[$meta])*
        $function_vis fn $function($pool_name: Option<$pool>) -> $returned {
            vec![
                $( $crate::generic::typed_access_row($resource, $access), )+
            ]
        }
    };
}

#[macro_export]
macro_rules! daf_row {
    ($resource:expr, access = $access:expr $(,)?) => {
        $crate::generic::typed_access_row($resource, $access)
    };
}
