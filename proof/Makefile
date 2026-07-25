JAR   := tools/tla2tools.jar
SPECS := MembershipOfferStanding Pledge EpisodeLease ActIntentPermit \
         MandateChain BudgetConservation AuthorityJournal \
         GreenfieldEnablement SubordinateReservation

all: $(SPECS)

$(SPECS): $(JAR)
	./check.sh $@

parity:
	python3 check-descriptors.py

full: all parity

$(JAR):
	mkdir -p tools
	curl -sL -o $(JAR) https://github.com/tlaplus/tlaplus/releases/latest/download/tla2tools.jar

.PHONY: all parity full $(SPECS)
