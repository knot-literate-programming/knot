"""
Setup script for knot Python package
"""

from setuptools import setup, find_packages

setup(
    name="knot",
    version="0.1.0",
    description="Python support for Knot literate programming with Typst",
    author="Knot Team",
    packages=find_packages(),
    python_requires=">=3.7",
    install_requires=[
        # No hard dependencies - matplotlib, pandas, plotnine are optional
    ],
    extras_require={
        'matplotlib': ['matplotlib>=3.0'],
        'pandas': ['pandas>=1.0'],
        'plotnine': ['plotnine>=0.8'],
        'all': ['matplotlib>=3.0', 'pandas>=1.0', 'plotnine>=0.8'],
    },
    classifiers=[
        "Development Status :: 3 - Alpha",
        "Intended Audience :: Developers",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.7",
        "Programming Language :: Python :: 3.8",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
    ],
)
