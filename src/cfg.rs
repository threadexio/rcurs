macro_rules! cfg_std {
	($($item:item)*) => {
		$(
			#[cfg(feature = "std")]
			$item
		)*
	};
}

pub(crate) use cfg_std;
