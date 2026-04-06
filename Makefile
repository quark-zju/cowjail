bench:
	@cargo run -q --release -- _kill && cargo run -q --release -- run /bin/bash -c 'echo COLD && time rg -l foobar /usr/share/man/; echo; echo WARM && time rg -l foobar /usr/share/man/' || true

.PHONY: bench
