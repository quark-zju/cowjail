bench:
	@cargo run -q --release -- _kill && cargo run -q --release -- run /bin/bash -c 'time rg -l foobar /usr/share/man/' || true

.PHONY: bench
