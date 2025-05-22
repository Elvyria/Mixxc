.DEFAULT_GOAL := release

prefix ?= /usr/local
bindir ?= ${prefix}/bin
datarootdir ?= ${prefix}/share
datadir ?= ${datarootdir}
mandir ?= ${datarootdir}/man
man1dir ?= ${mandir}/man1

name = $(shell sed -nE 's/name *?= *?"(.+)"/\1/p' ./Cargo.toml)
ifdef CARGO_TARGET_DIR
	target = ${CARGO_TARGET_DIR}
else
	target = ./target
endif

release:
	$(MAKE) clean
	cargo build --locked --release

debug:
	cargo build --locked

clean:
	cargo clean --package ${name}

install:
	test -d ${target}/release
	install -m 0755 -s ${target}/release/${name} ${bindir}/${name}
	install -m 0755 -d ${datadir}/${name}
	install -m 0644 $(wildcard ./style/*.scss) $(wildcard ${target}/release/build/${name}-*/out/*.css) ${datadir}/${name}
	install -m 0755 -d ${man1dir}
	gzip -9 -c ./doc/mixxc.1 > ${man1dir}/${name}.1.gz
	chmod 0644 ${man1dir}/${name}.1.gz
	mandb --no-purge --quiet

uninstall:
	rm ${bindir}/${name}
	rm -f $(wildcard ${datadir}/${name}/*.css ${datadir}/${name}/*.scss)
	rmdir ${datadir}/${name}
	rm ${man1dir}/${name}.1.gz
	mandb --quiet
