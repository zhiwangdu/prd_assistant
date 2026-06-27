FROM python:3.12-slim

ENV PYTHONUNBUFFERED=1
WORKDIR /opt/selftest

COPY requirements.txt /opt/selftest/requirements.txt
RUN pip install --no-cache-dir -r /opt/selftest/requirements.txt

COPY . /opt/selftest

CMD ["python", "/opt/selftest/runner.py"]
