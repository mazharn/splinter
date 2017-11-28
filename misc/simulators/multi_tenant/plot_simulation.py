#!/usr/bin/python

# Copyright (c) 2017 University of Utah
#
# Permission to use, copy, modify, and distribute this software for any
# purpose with or without fee is hereby granted, provided that the above
# copyright notice and this permission notice appear in all copies.
#
# THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR(S) DISCLAIM ALL WARRANTIES
# WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
# MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL AUTHORS BE LIABLE FOR
# ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
# WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
# ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
# OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

import numpy
import matplotlib.pyplot as plt

"""Dictionary mapping column names to column index.
"""
columnDict = {
    "Dist"     : 0,
    "ProcTime" : 1,
    "CSwitch"  : 2,
    "Tenants"  : 3,
    "Thrpt"    : 4,
}

def plotGraph(samples, procTime):
    """Plot throughput vs tenants for a particular processing time.

    @param samples: A list of samples for a particular processing time.
                    Each sample should have a set of columns specified in
                    columnDict.

    @param procTime: The processing time @samples was collected for.
    @type  procTime: C{str}
    """
    # Get the set of unique tenants for the x-axis.
    tenants = sorted(set([int(sample[columnDict["Tenants"]]) \
                          for sample in samples]))

    # Get the set of unique context switch times.
    cswitchTimes = set([sample[columnDict["CSwitch"]] for sample in samples])

    # Plot the graph.
    plt.close()
    xshift = -0.1
    xaxis = numpy.arange(len(tenants))
    yaxis = numpy.logspace(3, 6, num=4)
    for cswitch in cswitchTimes:
        # Filter samples, and sort by number of tenants.
        filtered = [sample for sample in samples \
                    if sample[columnDict["CSwitch"]] == cswitch]
        filtered = sorted(filtered, key = \
                       lambda sample: int(sample[columnDict["Tenants"]]))

        # Get the throughput for the yaxis.
        thrpt = [sample[columnDict["Thrpt"]] for sample in filtered]

        # Plot the graph. Throughput vs Tenants.
        plt.bar(xaxis+xshift, thrpt, width=0.2, align='center',
                label=str(float(cswitch) / 1000) + ' ' + r'$\mu$' + 'sec')
        xshift += 0.2

    # Format the graph.
    plt.xticks(xaxis, tenants)
    plt.xlabel('Number of tenants')

    # plt.yscale('log')
    # plt.yticks(yaxis)
    plt.ylabel('Throughput (Requests per second) in Log Scale')

    plt.title('Processing time of ' + str(int(procTime) / 1000) + \
              ' ' + r'$\mu$' + 'sec per extension.')
    plt.legend(ncol=len(cswitchTimes))

    # Save the graph.
    plt.savefig('throughput_procTime' + str(procTime) + 'ns.pdf')

    return

def plotAllGraphs(samples, dist='Zipf'):
    """Plot throughput vs tenants graphs for the given data.

    @param samples: A list consisting of the samples read from a data file.
                    Each sample should contain the set of columns specified
                    in columnDict.

    @param dist: A filter on the "Dist" field/column in @samples. Only points
                 with this value will be plotted on the graph.
    @type  dist: C{str}
    """
    # Filter out any non dist samples.
    samples = [sample for sample in samples \
               if sample[columnDict["Dist"]] == dist]

    # Get the set of unique processing times.
    procTimes = set([sample[columnDict["ProcTime"]] for sample in samples])

    # Plot one graph for each processing time.
    for procTime in procTimes:
        filtered = [sample for sample in samples if \
                    sample[columnDict["ProcTime"]] == procTime]
        plotGraph(filtered, procTime)

    return

if __name__ == '__main__':
    # Read samples generated by the simulator.
    samples = []
    with open("./samples.data") as dataFile:
        samples = [sample.split() for sample in dataFile if sample[0] != '#']

    # Plot the data.
    plotAllGraphs(samples)
