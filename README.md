<p align="center">
  <a href="https://hypercube-lab.github.io">
    <img alt="HyperCube" src="https://i.imgur.com/N4NsDhi.png" width="250" />
  </a>
</p>

[![HyperCube crate](https://img.shields.io/crates/v/hypercube.svg)](https://crates.io/crates/hypercube)
[![HyperCube documentation](https://docs.rs/hypercube-lab/badge.svg)](https://docs.rs/hypercu)
[![Build status](https://badge.buildkite.com/8cc350de251d61483db98bdfc895b9ea0ac8ffa4a32ee850ed.svg?branch=master)](https://buildkite.com/hypercube-lab/hypercube/builds?branch=master)
[![codecov](https://codecov.io/gh/hypercube-lab/hypercube/branch/master/graph/badge.svg)](https://codecov.io/gh/hypercube-lab/hypercube)


# What is HyperCube

HyperCube is a free and open source blockchain project for everyone to use.


[HyperCube Wiki](https://github.com/hypercube-lab/hypercube/wiki)

[HyperCube Whitepaper](https://github.com/hypercube-lab/hypercube/blob/main/HyperCube.pdf)



# What does HyperCube Do?

HyperCube is an Ethereum 2-layer solution based on proof of POD dedication and an independent public chain.

# What Consensus Algorithm does HyperCube use?

HyperCube's network is an independent public chain written in Rust language based on PoD consensus.

PoD is a new network consensus algorithm based on a hybrid consensus of PoW (ETHash) and PoS (Dedication Formula), which determines the blockchain network accounting privileges from multi-dimensional parameters such as time, network coupling, community participation, and online time.

The PoD consensus + XPZ public chain helps Ethereum increase network transaction speed and reduce Gas fees. At the same time, the introduction of the EVERNET permanent storage network can provide decentralized permanent storage space for financial infrastructure, the Internet of Things, and the Internet.

# Who is behind HyperCube?

HyperCube is developed by HyperCube Lab. HyperCube Lab is responsible for designing, coding and maintaining the HyperCube code base. 

HyperCube Lab is a DAO organization which is funded and guided by Apex Dex Foundation (ADF). 

ADF is a non-profit organization dedicated to supporting blockchain and other technology for the well-being of entire human race.

#  Origin of HyperCube

The XPZ development team is guided by multiple Turing Award winners as consultants, and the Ethereum technical team guides the development. Grayscale and Coinbase fragments. XPZ, under the joint promotion of many geek technology enthusiasts and teams around the world, actively explores the new Ethereum The road to expansion.

The XPZ (full name HyperCube) public chain supports large-scale, multi-threaded, multi-concurrent network computing and storage capabilities such as NFT casting, social tokens, DeFi, and financial applications.

# Who to Participate in HyperCube?

As HyperCube is under active development, and is going to release a testnet, for now the eaiest way to participate is running an HyperCube node.

#  HyperCube Node Rig

All the underlying infrastructure of HyperCube is powered by nodes, aka miners. User can use household machine to run a XPZ node. All XPZ miner is based on the dual-core mining mode of Nvdia and AMD. Based on the special structure of PoD, XPZ can conduct joint mining with ETH, which saves energy and achieves the goal of carbon neutrality on the chain.

After XPZ upgrades the perpetual storage network, large-scale on-chain low-cost storage can be realized, which provides a guarantee for permanent storage of data by facilitating hard disk storage space.


#  Core Advantages of HyperCube

* Support game financial GameFi, chain games, decentralized financial DeFi, XPZ system built-in Athena SDK, can help develop the rapid development of GameFi and DeFi products.

* XPZ core provides EVM general XVM (XPZ virtual machine), which is faster than EVM and enables Ethereum developers to get started quickly

* Low gas fee, fast response

* Athena SDK can provide fast casting NFT, low-cost digital art creation

* Anonymous social application. HyperCube provides an inhouse built application called QuntumnChat, which provides Decentralized IDentifier, On-Chain Messaging, QuntumnChat supports anonymous social, OTC and token storage, facilitating quick realization of NFT creators, and supporting the issuance of social tokens and personal NFT works

* XPZ supports a hybrid on-chain transaction engine based on order books and automated market makers, which can help DeFi to be implemented on a large scale

* Strong academic background: PoD and XPZ core technologies have passed peer review and will be released in international academic conferences soon

* Endorsed by IEEE and other international blockchain standard organizations

* Traditional American capital includes endorsement

# Contact

For academia inquiries, please feel free to contact us at hypercube-lab@apexdex.fund.

For general inquiries, please feel to contact us at team@hypercube-lab.org

For business inquiries, please contact at info@sqtech.com

# Future of HyperCube

HyperCube is designed as the next generation infrascture for Metaverse and AI. 

In order to prompot HyperCube technology and making mass adoption of blockchain technology possbile, SuperFulx Quntumn (SQ) is formed by Apex Dex Foundation (ADF). SQ is not actively researching various key technology using blockchain technology, for which will be disclosed 

In our vision, HyperCube will be widely used as a key infrastructure for various industries, like Finance, Manufacturing, Transportation, Entertainment and Goverment, providing vital support of computing power and storage capability.

# Vault

HyperCube will pre-farm a 210 million of XPZ at network launch, these 210 million XPZ will be stored in Vault.

The purpose of these 210 million XPZ is to help stabilize and grow the XPZ economy through DAO governance model. For the 210 million XPZ, 90% will be frozen until 2031.

# Regarding ICO

HyperCube has no ICO plans.

SQ wants the company's equity listed on an American exchange. We can make enforceable representations about how SQ intends to utilize the Vault using corporate controls. XPZ is a utility token, not an investment. So when mainnet launches, SQ aims to undertake an SEC registered stock IPO.

***

  
# Code Coverage

To generate code coverage statistics:

```bash
$ scripts/coverage.sh
$ open target/cov/lcov-local/index.html
```

Why coverage? While most see coverage as a code quality metric, we see it primarily as a developer
productivity metric. When a developer makes a change to the codebase, presumably it's a *solution* to
some problem.  Our unit-test suite is how we encode the set of *problems* the codebase solves. Running
the test suite should indicate that your change didn't *infringe* on anyone else's solutions. Adding a
test *protects* your solution from future changes. Say you don't understand why a line of code exists,
try deleting it and running the unit-tests. The nearest test failure should tell you what problem
was solved by that code. If no test fails, go ahead and submit a Pull Request that asks, "what
problem is solved by this code?" On the other hand, if a test does fail and you can think of a
better way to solve the same problem, a Pull Request with your solution would most certainly be
welcome! Likewise, if rewriting a test can better communicate what code it's protecting, please
send us that patch!

# Disclaimer

All claims, content, designs, algorithms, estimates, roadmaps,
specifications, and performance measurements described in this project
are done with the HyperCube Lab's ("XPZ Lab") good faith efforts. It is up to
the reader to check and validate their accuracy and truthfulness.
Furthermore nothing in this project constitutes a solicitation for
investment.

Any content produced by XPZ Lab or developer resources that HyperCube Lab provides, are
for educational and inspiration purposes only. HyperCube Lab does not encourage,
induce or sanction the deployment, integration or use of any such
applications (including the code comprising the HyperCube blockchain
protocol) in violation of applicable laws or regulations and hereby
prohibits any such deployment, integration or use. This includes use of
any such applications by the reader (a) in violation of export control
or sanctions laws of the United States or any other applicable
jurisdiction, (b) if the reader is located in or ordinarily resident in
a country or territory subject to comprehensive sanctions administered
by the U.S. Office of Foreign Assets Control (OFAC), or (c) if the
reader is or is working on behalf of a Specially Designated National
(SDN) or a person subject to similar blocking or denied party
prohibitions.

The reader should be aware that U.S. export control and sanctions laws
prohibit U.S. persons (and other persons that are subject to such laws)
from transacting with persons in certain countries and territories or
that are on the SDN list. As a project based primarily on open-source
software, it is possible that such sanctioned persons may nevertheless
bypass prohibitions, obtain the code comprising the HyperCube blockchain
protocol (or other project code or applications) and deploy, integrate,
or otherwise use it. Accordingly, there is a risk to individuals that
other persons using the HyperCube blockchain protocol may be sanctioned
persons and that transactions with such persons would be a violation of
U.S. export controls and sanctions law. This risk applies to
individuals, organizations, and other ecosystem participants that
deploy, integrate, or use the HyperCube blockchain protocol code directly
(e.g., as a node operator), and individuals that transact on the HyperCube
blockchain through light clients, third party interfaces, and/or qtc
software.



