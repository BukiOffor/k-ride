# Renting Service Proposal

## Overview

Add Renting Service as a separate booking flow within K-Ride. Customers can reserve a car with a driver for a minimum of 24 hours, supplied by verified organizations.

The backend already has users, cars, driver verification, rides, and scheduled bookings. Reuse these foundations while adding organization management and dedicated rental reservations.

This document is a proposal. The product rules below should be agreed before implementation.

## Organization and Driver Registration

Each organization has an owner or manager who can:

- Register the organization and submit verification documents.
- Invite drivers or approve drivers requesting to join.
- Register vehicles and submit their documents.
- Set rental prices and availability.
- Accept bookings and assign drivers.

Drivers can register themselves and request membership in an organization. K-Ride administrators approve organizations, drivers, and vehicles before they become bookable.

Organization permissions must be scoped so that managers can access only their own fleet and bookings.

## Rental Listings

For the first version, customers choose a specific car with a driver included. The organization assigns an eligible driver before confirming the reservation.

Each listing shows:

- Photos, model, passenger capacity, and luggage capacity.
- Organization name and service area.
- Daily price and what it includes.
- Included driver hours and mileage.
- Fuel, overtime, overnight, and out-of-town charges.
- Cancellation terms.

This approach gives organizations flexibility when a particular driver is unavailable. If choosing a specific driver is central to the service, an alternative is a named car-and-driver package.

## Customer Booking Journey

1. Enter pickup location, start time, and end time.
2. View available cars and an estimated total.
3. Select a car and provide an itinerary or special requirements.
4. Request the booking.
5. Receive confirmation after the organization assigns a driver and payment succeeds.
6. Start the rental using a verification PIN.
7. Complete the rental and rate the service.

For launch, require organization approval before confirmation. Instant booking can follow once availability calendars are reliable. Booking requests should expire so customers are not left waiting indefinitely.

## Duration, Pricing, and Driver Hours

| Item | Suggested rule |
| --- | --- |
| Minimum duration | 24 consecutive hours |
| Billing | Each started 24-hour period counts as a rental day |
| Driver service | A clearly stated number of working hours per day |
| Additional charges | Disclosed upfront; customer approval required for extras |
| Extensions | Allowed only when both car and driver remain available |
| Overnight arrangements | Explicitly included or priced separately |

A 24-hour reservation does not mean the driver works continuously for 24 hours. Customers must see the included driver service hours before booking.

## Availability and Conflict Prevention

Both the vehicle and driver must be available for the reservation period.

A confirmed rental must block overlapping rentals, scheduled rides, and ordinary ride assignments. Maintenance, driver leave, and preparation time must also block availability.

Check availability again when confirming or extending a booking. Protect reservations at the database level to prevent two customers from reserving the same resource simultaneously.

## Backend Structure

Add a dedicated rental module alongside the existing ride and bus modules, with records for:

- Organizations and memberships.
- Rental listings, prices, and policies.
- Vehicle and driver availability blocks.
- Rental quotes and bookings.
- Driver assignments.
- Payments, refunds, and organization payouts.
- Extensions, additional charges, and booking history.

Preserve the agreed price and policies on each booking so later listing changes cannot alter an existing reservation.

Avoid tying rental eligibility to the community-specific fields currently present in driver onboarding.

## First Release Scope

Include:

- Verified organizations, drivers, and vehicles.
- Car-with-driver listings.
- Daily pricing and clear service terms.
- Vehicle and driver availability calendars.
- Organization approval of reservation requests.
- Payments and cancellations.
- Administrative oversight.

Defer self-drive rentals, hourly bookings, dynamic pricing, and complex driver rotations until later releases.

## Main Product Decision

Decide whether customers reserve a car with an assigned driver or choose a specific car-and-driver pair.

The recommendation is to reserve a car with an assigned driver. This makes bookings easier for organizations to fulfil reliably while still showing customers their assigned driver before the rental begins.
